# AI Switch Provider Switching B2.3 Gemini CLI Real Adapter Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add explicit real provider switching for Gemini CLI by writing documented Gemini CLI user `settings.json` through the existing backend path resolution, backup-aware atomic writer, snapshot, rollback, tray, and target-state pipeline.

**Architecture:** B2.3 adds `adapters::gemini_config` and extends provider switch real-mode dispatch from Codex/OpenCode to Codex/OpenCode/Gemini CLI. The adapter is intentionally conservative: it writes `model.name` and safe `aiSwitch.activeProvider` metadata only, because public Gemini CLI settings do not define an arbitrary OpenAI-compatible provider block.

**Tech Stack:** Tauri 2, React 18, TypeScript, Vite, Vitest, Testing Library, Rust, sqlx SQLite, serde, serde_json, chrono, uuid, tokio.

## Global Constraints

- B2.3 must keep sandbox switching unchanged.
- B2.3 must keep Codex and OpenCode real switching unchanged.
- B2.3 must accept `mode = "real"` for `codex`, `opencode`, and `gemini_cli` only.
- B2.3 must not write real configs for Claude Code, Claude Desktop, OpenClaw, or Hermes.
- B2.3 must resolve real Gemini CLI config paths in the backend only.
- B2.3 must write Gemini CLI config through `ConfigWriter::write_atomic_with_backup`.
- B2.3 must not write raw API keys, resolved secrets, or `secret_ref` into Gemini CLI config.
- B2.3 must not claim arbitrary OpenAI-compatible provider injection for Gemini CLI.

---

### Task 1: Add Gemini CLI Config Adapter

**Files:**
- Create: `src-tauri/src/adapters/gemini_config.rs`
- Modify: `src-tauri/src/adapters/mod.rs`

**Interfaces:**
- Consumes: `Provider`, `AppError`, `directories::BaseDirs`, `serde_json`
- Produces: `resolve_gemini_config_path() -> Result<PathBuf, AppError>`
- Produces: `resolve_gemini_config_path_with(custom_config: Option<&Path>, home_dir: &Path) -> Result<PathBuf, AppError>`
- Produces: `render_gemini_provider_config(path: &Path, provider: &Provider) -> Result<GeminiRenderedConfig, AppError>`
- Produces: `render_gemini_provider_config_from_str(path: &Path, existing: &str, provider: &Provider) -> Result<GeminiRenderedConfig, AppError>`

- [x] **Step 1: Confirm public Gemini CLI config facts**

Use public Gemini CLI package/docs only:

```powershell
npm view @google/gemini-cli homepage repository.url version description --json
```

Confirmed facts for the implementation:

- user settings path is `~/.gemini/settings.json`
- settings are JSON
- model setting is `model.name`
- `GEMINI_MODEL` and CLI flags can override settings, so the app writes a default rather than guaranteeing runtime override
- authentication uses login or environment variables, so the app must not write credentials

- [x] **Step 2: Write adapter tests**

Add tests in `src-tauri/src/adapters/gemini_config.rs` covering:

```rust
#[test]
fn resolves_custom_gemini_config_path() {
    let dir = tempdir().expect("tempdir");
    let custom_path = dir.path().join("settings.json");

    let path =
        resolve_gemini_config_path_with(Some(&custom_path), Path::new("C:/Users/example"))
            .expect("path");

    assert_eq!(path, custom_path);
}

#[test]
fn renders_model_and_preserves_unrelated_json() {
    let existing = r#"{"ui":{"theme":"dark"},"model":{"maxSessionTurns":5}}"#;

    let rendered = render_gemini_provider_config_from_str(
        Path::new("C:/Users/example/.gemini/settings.json"),
        existing,
        &provider(),
    )
    .expect("rendered");
    let parsed: Value = serde_json::from_str(&rendered.contents).expect("json");

    assert_eq!(rendered.model_id, "gemini-2.5-flash");
    assert_eq!(parsed["ui"]["theme"], "dark");
    assert_eq!(parsed["model"]["maxSessionTurns"], 5);
    assert_eq!(parsed["model"]["name"], "gemini-2.5-flash");
    assert_eq!(parsed["aiSwitch"]["activeProvider"]["id"], "Provider-1");
    assert_eq!(parsed["aiSwitch"]["activeProvider"]["envKey"], "GEMINI_API_KEY");
    assert!(!rendered.contents.contains("secret://provider/acme"));
}

#[test]
fn rejects_malformed_existing_json() {
    let error = render_gemini_provider_config_from_str(
        Path::new("C:/Users/example/.gemini/settings.json"),
        "{\"model\":",
        &provider(),
    )
    .expect_err("error");

    assert_eq!(error.code(), "validation.gemini_config_json");
}

#[test]
fn rejects_missing_model_id() {
    let mut provider = provider();
    provider.model_config_json = "{}".to_string();
    provider.target_options_json = "{}".to_string();

    let error = render_gemini_provider_config_from_str(
        Path::new("C:/Users/example/.gemini/settings.json"),
        "",
        &provider,
    )
    .expect_err("error");

    assert_eq!(error.code(), "validation.provider_model_required");
}
```

- [ ] **Step 3: Implement adapter**

Implement these exact behaviors:

- `GEMINI_CLI_SETTINGS` non-empty override wins for path resolution.
- Default path is `<home>/.gemini/settings.json`.
- Path must be absolute.
- Existing JSON must be an object.
- Model id resolution order is `target_options_json.gemini_cli.model`, `target_options_json.model`, `model_config_json.gemini_cli.model`, `model_config_json.default`, `model_config_json.model`.
- Env key resolution order is `target_options_json.gemini_cli.env_key`, `target_options_json.env_key`, default `GEMINI_API_KEY`.
- Render pretty JSON with trailing newline.

- [ ] **Step 4: Register adapter module**

Add to `src-tauri/src/adapters/mod.rs`:

```rust
pub mod gemini_config;
```

- [ ] **Step 5: Run adapter tests**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml gemini_config
```

Expected: Gemini adapter tests pass.

### Task 2: Extend Provider Switch Real Dispatch

**Files:**
- Modify: `src-tauri/src/services/provider_switch_service.rs`

**Interfaces:**
- Consumes: `render_gemini_provider_config`, `resolve_gemini_config_path`
- Produces: `ProviderSwitchService::switch_provider()` accepting `mode = "real"` for target key `gemini_cli`

- [ ] **Step 1: Extend path overrides**

Add `gemini: Option<PathBuf>` to `RealConfigPathOverrides`, and add a test helper:

```rust
#[cfg(test)]
pub async fn switch_provider_with_gemini_config_path(
    pool: &SqlitePool,
    paths: &AppPaths,
    request: ProviderSwitchRequest,
    gemini_config_path: PathBuf,
) -> Result<ProviderSwitchOutcome, AppError>
```

- [ ] **Step 2: Add Gemini dispatch branch**

In `switch_provider_real`, add target key `gemini_cli` and update unsupported-target message to mention Codex, OpenCode, and Gemini CLI.

- [ ] **Step 3: Add real Gemini helper**

Add `switch_provider_real_gemini` mirroring `switch_provider_real_opencode`, but using:

```rust
let rendered = render_gemini_provider_config(&path, &provider).await?;
let backup_dir = real_config_backup_dir(paths, &target)?;
ConfigWriter::write_atomic_with_backup(&rendered.path, &rendered.contents, &backup_dir).await
```

- [ ] **Step 4: Add backend tests**

Add tests in `provider_switch_service.rs`:

- success writes Gemini settings, records `switch_provider:real`, stores backup path, and updates provider state
- failure after Gemini path resolution records failed snapshot/state when model id is missing
- unsupported real target still rejects Claude

- [ ] **Step 5: Run provider switch tests**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml provider_switch_service
```

Expected: provider switch tests pass.

### Task 3: Expose Gemini CLI Real Action In UI And Tray

**Files:**
- Modify: `src/screens/ProvidersScreen.tsx`
- Modify: `src-tauri/src/tray.rs`
- Modify: `src/test/fixtures.ts`
- Modify: `tests/ProvidersScreen.test.tsx`

**Interfaces:**
- Consumes: target key `gemini_cli`
- Produces: Providers button label `Switch Gemini CLI config`
- Produces: tray real menu item for Gemini CLI

- [ ] **Step 1: Add UI label**

Update `realConfigTargetLabels`:

```ts
const realConfigTargetLabels: Record<string, string> = {
  codex: "Codex",
  gemini_cli: "Gemini CLI",
  opencode: "OpenCode",
};
```

- [ ] **Step 2: Add fixture status if missing**

Ensure `targetSwitchStatusesFixture` includes target id `target-gemini`, key `gemini_cli`, display name `Gemini CLI`.

- [ ] **Step 3: Add ProvidersScreen test**

Add a test that selects `target-gemini`, clicks `Switch Acme Provider Gemini CLI config`, and expects:

```ts
expect(switchTargetProvider).toHaveBeenCalledWith({
  target_app_id: "target-gemini",
  provider_id: "provider-1",
  mode: "real",
});
```

- [ ] **Step 4: Extend tray real target list**

Update `is_real_tray_target`:

```rust
matches!(target_key, "codex" | "gemini_cli" | "opencode")
```

Update tray count test to include Gemini CLI and expected count.

- [ ] **Step 5: Run frontend and tray tests**

Run:

```powershell
pnpm test:run -- tests/ProvidersScreen.test.tsx
cargo test --manifest-path src-tauri/Cargo.toml tray
```

Expected: frontend and tray tests pass.

### Task 4: Documentation And Verification

**Files:**
- Modify: `README.md`
- Modify: `docs/superpowers/plans/2026-07-13-ai-switch-provider-switching-b2-gemini-real.md`

**Interfaces:**
- Produces: README smoke section for B2.3 Gemini CLI real mode
- Produces: checked completion state in this plan

- [ ] **Step 1: Add README B2.3 notes**

Add a section after B2.2:

```markdown
## Provider Switching B2.3: Gemini CLI Real Mode

B2.3 adds explicit real provider switching for Gemini CLI. It writes Gemini CLI user settings at `~/.gemini/settings.json`, or a temporary `GEMINI_CLI_SETTINGS` path when set for safe smoke testing.

Gemini CLI real mode sets `model.name` and safe `aiSwitch.activeProvider` metadata. It does not write raw API keys, `secret_ref`, or arbitrary OpenAI-compatible base URLs.
```

- [ ] **Step 2: Run full verification**

Run:

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
pnpm test:run
pnpm typecheck
pnpm rust:check
pnpm rust:test
```

Expected: all pass.

- [ ] **Step 3: Mark plan complete**

Update all unchecked boxes in this plan to `[x]` after implementation and verification.

- [ ] **Step 4: Commit B2.3**

Run:

```powershell
git add README.md docs/superpowers/specs/2026-07-13-ai-switch-provider-switching-b2-gemini-real-design.md docs/superpowers/plans/2026-07-13-ai-switch-provider-switching-b2-gemini-real.md src-tauri/src/adapters/gemini_config.rs src-tauri/src/adapters/mod.rs src-tauri/src/services/provider_switch_service.rs src-tauri/src/tray.rs src/screens/ProvidersScreen.tsx src/test/fixtures.ts tests/ProvidersScreen.test.tsx
git commit -m "feat: add gemini cli real provider switching"
```

## Verification Commands

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
pnpm test:run
pnpm typecheck
pnpm rust:check
pnpm rust:test
```

## Safe Smoke

Use a temporary Gemini CLI settings path when manually smoking real mode:

```powershell
$env:GEMINI_CLI_SETTINGS = Join-Path $env:TEMP "ai-switch-gemini-smoke\settings.json"
pnpm tauri:dev
```

Expected:

- Providers screen shows `Switch Gemini CLI config` only when Gemini CLI is selected.
- Clicking it writes the temporary `GEMINI_CLI_SETTINGS` path.
- The file contains `model.name` and `aiSwitch.activeProvider`.
- The file does not contain raw API keys or `secret_ref`.
