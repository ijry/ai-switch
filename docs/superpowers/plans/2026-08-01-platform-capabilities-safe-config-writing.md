# Platform Capabilities and Safe Config Writing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make AI Switch truthful about seven-platform support and route every supported agent configuration mutation through recoverable, conflict-aware, atomic writes.

**Architecture:** Introduce a typed platform/capability domain shared by all Rust services and exposed to the frontend. Separate agent identity from API dialect, move the four existing native config renderers behind a production target-adapter registry, and commit their mutation plans through a snapshot-backed coordinator. Tauri and Web transports call the same services and return the same structured outcomes and errors.

**Tech Stack:** Rust 2021, Tauri 2, Tokio, SQLx/SQLite, Serde, SHA-256, Windows file APIs through `windows-sys`, React 18, TypeScript, TanStack Query, Vitest, Testing Library.

## Global Constraints

- Work directly on `main`; do not create a branch or worktree.
- Canonical platform IDs are exactly `codex`, `claude`, `gemini`, `grok`, `opencode`, `openclaw`, and `hermes`.
- Keep OpenCode, OpenClaw, and Hermes visible as partially supported.
- Do not implement native OpenCode, OpenClaw, or Hermes config adapters in this plan.
- Do not resolve or touch a Hermes config path during any AI Switch config-write operation.
- Unknown platform values fail closed; they never become Codex or receive OpenAI official semantics.
- OpenCode, OpenClaw, and Hermes generic routing/model testing is API-credential-only and requires an explicit base URL and API dialect.
- Supported config writes remain one-click safe direct writes; do not add a diff confirmation dialog.
- Do not expose config contents, backup contents, API keys, access tokens, refresh tokens, or route proxy keys through responses or logs.
- Do not add force overwrite, force rollback, or automatic snapshot pruning.
- Preserve unmanaged Codex, Claude, Gemini, and Grok configuration fields.
- Treat `src-tauri/Cargo.lock`, `tauri-dev.err`, and `tauri-dev.log.err` as pre-existing unrelated workspace changes unless the user explicitly expands scope.
- Commit commands in this plan are conditional: run them only after explicit user authorization to commit; otherwise leave changes uncommitted and continue with the next task.

## File Structure

### New Backend Files

- `src-tauri/src/models/platform.rs`: canonical platform IDs, API dialects, capability DTOs, and strict alias parsing.
- `src-tauri/src/models/config_snapshot.rs`: snapshot records, public summaries, write outcomes, and target config status DTOs.
- `src-tauri/src/services/platform_capability_service.rs`: immutable seven-platform capability matrix and guard methods.
- `src-tauri/src/services/config_write_service.rs`: path locks, snapshot-backed write groups, guarded rollback, and interrupted-operation reconciliation.
- `src-tauri/src/database/repositories/config_snapshot_repository.rs`: snapshot prepare/update/list/lookup queries.
- `src-tauri/src/database/repositories/target_state_repository.rs`: `target_app_states` upserts and reads.
- `src-tauri/src/adapters/route_config/mod.rs`: production adapter trait, registry, shared input, and inspection types.
- `src-tauri/src/adapters/route_config/codex.rs`: Codex TOML merge and inspection.
- `src-tauri/src/adapters/route_config/json_agent.rs`: Claude/Gemini/Grok JSON merge and inspection.
- `src-tauri/src/config_writer/platform.rs`: platform-specific link checks, replacement, metadata preservation, and parent sync.
- `src-tauri/src/commands/platform_commands.rs`: capability listing command.
- `src-tauri/migrations/202608010001_platform_capabilities_safe_writes.sql`: target-platform metadata and snapshot transaction columns/indexes.

### New Frontend Files

- `src/lib/platformCapabilities.ts`: capability lookup, enabled-state, and reason helpers.
- `src/lib/query/platformCapabilities.ts`: shared infinite-stale capability query.
- `src/components/platform/PlatformSupportBadge.tsx`: supported/partial badge.
- `src/lib/api/errors.ts`: structured transport error normalization.
- `src/lib/api/commandSupport.ts`: explicit desktop-only command allowlist.
- `tests/platformCapabilities.test.ts`: capability helper tests.
- `tests/TargetsScreen.test.tsx`: target status and rollback tests.
- `tests/DashboardScreen.test.tsx`: data-backed dashboard tests.
- `tests/OperationLogScreen.test.tsx`: persisted config-event tests.
- `tests/transport/command-contract.test.ts`: client/Tauri/Web command-name contract.

### Major Existing Files Modified

- `src-tauri/src/services/route_config_service.rs`: delegate rendering and mutation to adapters/coordinator.
- `src-tauri/src/config_writer/mod.rs`: expose conflict-aware byte-oriented atomic operations.
- `src-tauri/src/services/route_proxy_service.rs`: strict platform resolution and explicit API/official semantics.
- `src-tauri/src/services/route_model_test_service.rs`: capability- and dialect-aware request construction.
- `src-tauri/src/services/route_quota_service.rs`: reject unsupported quota operations before refresh/network work.
- `src-tauri/src/services/route_credential_service.rs`: central parsing and official-import capability guards.
- `src-tauri/src/services/cpa_import_service.rs`, `src-tauri/src/services/sub2api_import_service.rs`, `src-tauri/src/services/deeplink_service.rs`: central parsing and explicit capability checks.
- `src-tauri/src/database/repositories/target_repository.rs`: target-to-platform mapping, Grok target, and lookup by key.
- `src-tauri/src/services/target_service.rs`: real config status inspection.
- `src-tauri/src/commands/target_commands.rs`: target status, snapshot list, and rollback commands.
- `src-tauri/src/app_state.rs`, `src-tauri/src/lib.rs`, `src-tauri/src/server.rs`: shared config-write runtime state and command registration.
- `src-tauri/src/web/handlers/mod.rs`, `src-tauri/src/web/router.rs`: parity handlers and structured errors.
- `src/lib/api/types.ts`, `src/lib/api/client.ts`: capability/snapshot/status contracts.
- `src/screens/AccountsScreen.tsx`, `src/screens/TargetsScreen.tsx`, `src/screens/DashboardScreen.tsx`, `src/screens/OperationLogScreen.tsx`, `src/screens/ProvidersScreen.tsx`: truthful capability-aware UI.
- `README.md`: public support matrix and safe-write behavior.

---

### Task 1: Typed Platform Domain and Capability Registry

**Files:**
- Create: `src-tauri/src/models/platform.rs`
- Create: `src-tauri/src/services/platform_capability_service.rs`
- Modify: `src-tauri/src/models/mod.rs:1`
- Modify: `src-tauri/src/services/mod.rs:1`
- Test: inline tests in both new Rust modules

**Interfaces:**
- Consumes: approved matrix in `docs/superpowers/specs/2026-08-01-platform-capabilities-safe-config-writing-design.md`.
- Produces: `PlatformId::parse`, `PlatformId::as_str`, `PlatformId::default_api_credential_dialect`, `ApiDialect::parse`, `PlatformOperation`, `PlatformCapability`, and `PlatformCapabilityService::{list,get,require}`.

- [ ] **Step 1: Write strict platform and dialect parsing tests**

Add tests that encode the accepted alias set and reject substring fallback:

```rust
#[test]
fn parses_only_explicit_platform_aliases() {
    assert_eq!(PlatformId::parse("claude-code").unwrap(), PlatformId::Claude);
    assert_eq!(PlatformId::parse("x.ai").unwrap(), PlatformId::Grok);
    assert_eq!(PlatformId::parse("OpenClaw").unwrap(), PlatformId::OpenClaw);
    assert!(PlatformId::parse("my-claude-wrapper").is_err());
    assert!(PlatformId::parse("unknown-provider").is_err());
}

#[test]
fn parses_supported_api_dialect_aliases() {
    assert_eq!(ApiDialect::parse("openai-responses").unwrap(), ApiDialect::OpenAiResponses);
    assert_eq!(ApiDialect::parse("anthropic-messages").unwrap(), ApiDialect::Anthropic);
    assert!(ApiDialect::parse("automatic").is_err());
}
```

- [ ] **Step 2: Run the new parsing tests and verify they fail**

Run: `cd src-tauri && cargo test platform::tests --lib`

Expected: FAIL because `PlatformId` and `ApiDialect` do not exist.

- [ ] **Step 3: Implement the canonical domain types**

Define the exact enums and normalization boundary:

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum PlatformId {
    Codex,
    Claude,
    Gemini,
    Grok,
    OpenCode,
    OpenClaw,
    Hermes,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ApiDialect {
    #[serde(rename = "openai")]
    OpenAi,
    #[serde(rename = "openai-responses")]
    OpenAiResponses,
    #[serde(rename = "anthropic")]
    Anthropic,
    #[serde(rename = "gemini")]
    Gemini,
}
```

Normalize by trimming, lowercasing, and replacing spaces/hyphens with `_`, then match only this alias table:

```text
codex: codex, openai, chatgpt
claude: claude, anthropic, claude_code, claude_desktop
gemini: gemini, google, gemini_cli
grok: grok, xai, x_ai, x.ai
opencode: opencode, open_code
openclaw: openclaw, open_claw
hermes: hermes
```

Return this exact error shape for blank or unmatched values:

```rust
Err(AppError::Validation {
    code: "platform.unknown",
    message: "Platform is not recognized".to_string(),
    details: Some(value.trim().to_string()),
    recoverable: true,
})
```

Add `default_api_credential_dialect() -> Option<ApiDialect>` with Codex/Grok -> `OpenAi`, Claude -> `Anthropic`, Gemini -> `Gemini`, and the three partial platforms -> `None`.

- [ ] **Step 4: Write the seven-platform capability matrix test**

```rust
#[test]
fn capability_matrix_matches_phase_a_contract() {
    let matrix = PlatformCapabilityService::list();
    assert_eq!(matrix.len(), 7);
    let hermes = matrix.iter().find(|item| item.platform == PlatformId::Hermes).unwrap();
    assert_eq!(hermes.support_level, SupportLevel::Partial);
    assert_eq!(hermes.operations.config_write.availability, CapabilityAvailability::Unavailable);
    assert_eq!(hermes.operations.generic_api_routing.availability, CapabilityAvailability::Partial);
    assert_eq!(hermes.operations.official_account_routing.availability, CapabilityAvailability::Unavailable);

    let gemini = matrix.iter().find(|item| item.platform == PlatformId::Gemini).unwrap();
    assert_eq!(gemini.operations.config_write.availability, CapabilityAvailability::Supported);
    assert_eq!(gemini.operations.official_quota.availability, CapabilityAvailability::Unavailable);
}
```

- [ ] **Step 5: Implement capability DTOs, matrix, and guards**

Use explicit structs instead of a string-keyed map:

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityAvailability { Supported, Partial, Unavailable }

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SupportLevel { Supported, Partial }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilityRule {
    pub availability: CapabilityAvailability,
    pub reason_code: Option<String>,
    pub credential_kinds: Vec<String>,
    pub requires_base_url: bool,
    pub requires_api_dialect: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlatformOperations {
    pub route_credentials: CapabilityRule,
    pub generic_api_routing: CapabilityRule,
    pub config_write: CapabilityRule,
    pub official_import: CapabilityRule,
    pub official_account_routing: CapabilityRule,
    pub deeplink_import: CapabilityRule,
    pub official_quota: CapabilityRule,
    pub model_test: CapabilityRule,
    pub terminal_launch: CapabilityRule,
    pub session_resume: CapabilityRule,
}
```

For OpenCode/OpenClaw/Hermes, set `generic_api_routing` and `model_test` to partial with `credential_kinds: ["api"]`, `requires_base_url: true`, and `requires_api_dialect: true`. Set native config, official import/routing, Deeplink, and quota to unavailable. `require` rejects only unavailable operations with `capability.unavailable` and returns partial rules to the caller for constraint validation.

- [ ] **Step 6: Run focused tests**

Run: `cd src-tauri && cargo test platform --lib`

Expected: PASS for parsing, serialization, matrix, and guard behavior.

- [ ] **Step 7: Checkpoint Task 1**

Run: `git diff --check -- src-tauri/src/models src-tauri/src/services`

If commits are authorized:

```powershell
git add src-tauri/src/models/platform.rs src-tauri/src/models/mod.rs src-tauri/src/services/platform_capability_service.rs src-tauri/src/services/mod.rs
git commit -m "feat: define platform capability registry"
```

### Task 2: Centralize Platform Parsing and Capability Guards

**Files:**
- Modify: `src-tauri/src/services/route_pool_service.rs:255`
- Modify: `src-tauri/src/services/route_credential_service.rs:27,38,89,353`
- Modify: `src-tauri/src/services/route_quota_service.rs:42,50,1066`
- Modify: `src-tauri/src/services/cpa_import_service.rs:265`
- Modify: `src-tauri/src/services/sub2api_import_service.rs:507`
- Modify: `src-tauri/src/services/deeplink_service.rs:139`
- Test: inline tests in the modified service modules

**Interfaces:**
- Consumes: `PlatformId`, `PlatformOperation`, `PlatformCapabilityService::require` from Task 1.
- Produces: all credential/pool/import/quota/Deeplink entry points use one exact canonicalizer and fail before unsupported work.

- [ ] **Step 1: Add regression tests for unknown and unsupported inputs**

Add focused assertions:

```rust
#[test]
fn route_pool_does_not_default_unknown_platform_to_codex() {
    let error = PlatformId::parse("custom-agent").unwrap_err();
    assert!(matches!(error, AppError::Validation { code: "platform.unknown", .. }));
}

#[tokio::test]
async fn hermes_official_import_is_rejected_before_creating_a_batch() {
    let pool = create_memory_pool().await.unwrap();
    run_migrations(&pool).await.unwrap();
    let error = RouteCredentialService::import_official_text(&pool, ImportOfficialTextInput {
        platform: "hermes".into(),
        text: "{}".into(),
        batch_name: Some("Hermes import".into()),
    }).await.unwrap_err();
    assert!(matches!(error, AppError::Validation { code: "capability.unavailable", .. }));
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM batches").fetch_one(&pool).await.unwrap();
    assert_eq!(count, 0);
}
```

Add quota and Deeplink tests proving Gemini quota and OpenCode/OpenClaw/Hermes Deeplinks return `capability.unavailable` without network or repository writes.

- [ ] **Step 2: Run focused service tests and verify failures**

Run: `cd src-tauri && cargo test route_pool_service --lib && cargo test route_credential_service --lib && cargo test route_quota_service --lib && cargo test deeplink_service --lib`

Expected: FAIL because the services still use local normalizers or permissive fallbacks.

- [ ] **Step 3: Replace local normalization functions**

Use the same pattern in pool and credential CRUD paths:

```rust
let platform = PlatformId::parse(&input.platform)?;
let platform_key = platform.as_str();
```

Delete `route_pool_service::normalize_platform`, `route_credential_service::normalize_platform`, and `route_quota_service::normalize_platform`. Update callers to pass canonical `platform.as_str()` to repositories.

For CPA/Sub2API raw-type matching, parse both sides and compare `PlatformId`; an unrecognized raw type is not equal to a known requested platform.

- [ ] **Step 4: Guard official imports, quota, and Deeplink mapping before side effects**

At the beginning of official import methods:

```rust
let platform = PlatformId::parse(&input.platform)?;
PlatformCapabilityService::require(platform, PlatformOperation::OfficialImport)?;
let batch_id = ensure_required_batch(pool, input.batch_name).await?;
```

At quota entry points, load/parse the platform and require `OfficialQuota` before token refresh or HTTP-client creation. In `deeplink_service::map_app`, parse the app, require `DeeplinkImport`, and map dialect explicitly:

```rust
let dialect = match platform {
    PlatformId::Codex => ApiDialect::OpenAiResponses,
    PlatformId::Claude => ApiDialect::Anthropic,
    PlatformId::Gemini => ApiDialect::Gemini,
    PlatformId::Grok => ApiDialect::OpenAi,
    _ => unreachable!("capability guard rejects unsupported Deeplink platforms"),
};
```

- [ ] **Step 5: Run focused and repository tests**

Run: `cd src-tauri && cargo test route_pool_service --lib && cargo test route_credential_service --lib && cargo test route_quota_service --lib && cargo test deeplink_service --lib`

Expected: PASS; existing Grok aliases still canonicalize, while unknown and unsupported values fail closed.

- [ ] **Step 6: Checkpoint Task 2**

Run: `git diff --check -- src-tauri/src/services`

If commits are authorized:

```powershell
git add src-tauri/src/services/route_pool_service.rs src-tauri/src/services/route_credential_service.rs src-tauri/src/services/route_quota_service.rs src-tauri/src/services/cpa_import_service.rs src-tauri/src/services/sub2api_import_service.rs src-tauri/src/services/deeplink_service.rs
git commit -m "fix: reject unsupported platform operations"
```

### Task 3: Remove Proxy and Model-Test Fallback Semantics

**Files:**
- Modify: `src-tauri/src/services/route_proxy_service.rs:782,907,935,1195,1264,2389`
- Modify: `src-tauri/src/services/route_model_test_service.rs:39,238,568`
- Test: inline tests in both services

**Interfaces:**
- Consumes: `PlatformId`, `ApiDialect`, capability rules from Tasks 1-2.
- Produces: strict proxy platform resolution; API requests use explicit dialect constraints; official requests are unavailable for partial platforms.

- [ ] **Step 1: Write proxy identity and official-routing regression tests**

```rust
#[tokio::test]
async fn resolve_platform_preserves_hermes_proxy_key_identity() {
    let pool = create_memory_pool().await.unwrap();
    run_migrations(&pool).await.unwrap();
    RouteProxyKeyRepository::ensure_platform_key(&pool, "hermes", "sk-ai-switch-hermes").await.unwrap();
    let state = ProxyAppState {
        pool,
        key_cache: Arc::new(Mutex::new(RouteProxyKeyCache::default())),
    };
    let platform = resolve_platform(&state, &HeaderMap::new(), Some("sk-ai-switch-hermes")).await.unwrap();
    assert_eq!(platform, PlatformId::Hermes);
}

#[tokio::test]
async fn resolve_platform_without_key_or_header_fails_closed() {
    let pool = create_memory_pool().await.unwrap();
    run_migrations(&pool).await.unwrap();
    let state = ProxyAppState {
        pool,
        key_cache: Arc::new(Mutex::new(RouteProxyKeyCache::default())),
    };
    let error = resolve_platform(&state, &HeaderMap::new(), None).await.unwrap_err();
    assert!(matches!(error, AppError::Validation { code: "route_proxy.platform_unresolved", .. }));
}
```

Add request-builder tests proving an official Hermes credential fails with `capability.unavailable`, and an API Hermes credential without `interface_format` fails with `validation.api_dialect_required` before URL construction.

- [ ] **Step 2: Write model-test partial-platform tests**

Create one Hermes API credential with explicit `base_url` and `interface_format: "openai"`; assert `build_model_test_request` selects `/chat/completions`. Create one Hermes official credential and assert request construction returns `capability.unavailable`. Create one Hermes API credential without `interface_format` and assert `validation.api_dialect_required`.

- [ ] **Step 3: Run focused tests and verify failures**

Run: `cd src-tauri && cargo test resolve_platform --lib && cargo test partial_platform --lib`

Expected: FAIL because proxy-key identities and unknown headers still normalize to Codex/OpenAI.

- [ ] **Step 4: Make proxy platform resolution typed and explicit**

Change the resolver contract:

```rust
async fn resolve_platform(
    state: &ProxyAppState,
    headers: &HeaderMap,
    inbound_key: Option<&str>,
) -> Result<PlatformId, AppError>
```

Resolution order is local proxy key, then `x-ai-switch-platform`. Parse both through `PlatformId::parse`. Remove path-based pool identity and delete `normalize_route_platform`; request paths may still be normalized after the selected credential's explicit dialect is known.

- [ ] **Step 5: Require explicit dialects for partial-platform API credentials**

In `build_api_upstream_request`, parse platform first and resolve the dialect with this rule:

```rust
let raw_dialect = string_value(config, "interface_format");
let dialect = match raw_dialect {
    Some(value) => ApiDialect::parse(value).map_err(|error| error.to_string())?,
    None if matches!(platform, PlatformId::OpenCode | PlatformId::OpenClaw | PlatformId::Hermes) => {
        return Err("validation.api_dialect_required".to_string());
    }
    None => platform.default_api_credential_dialect().ok_or_else(|| {
        "validation.api_dialect_required".to_string()
    })?,
};
```

Keep `base_url` mandatory. Match headers and request paths on `ApiDialect`, not platform identity.

- [ ] **Step 6: Reject unsupported official routing and remove default official URL fallback**

Before token handling in `build_official_upstream_request`, require `PlatformOperation::OfficialAccountRouting`. Replace `default_official_base_url` with a `Result` that has explicit Codex/Claude/Gemini/Grok arms and no wildcard arm.

Change `route_model_test_service::interface_format_for` to return `Result<ApiDialect, AppError>`. API credentials follow the same partial-platform explicit-dialect rule; official credentials require `OfficialAccountRouting`. Do not construct a fallback OpenAI request for an error outcome.

When a partial platform runs a pool-level model test, filter the candidate list to `kind == "api"` before cursor selection. A single-account request for a legacy official credential returns `capability.unavailable`; it never silently tests that credential with OpenAI semantics.

- [ ] **Step 7: Run proxy and model-test suites**

Run: `cd src-tauri && cargo test route_proxy_service --lib && cargo test route_model_test_service --lib`

Expected: PASS, including existing Codex/Claude/Gemini/Grok request-shape coverage.

- [ ] **Step 8: Checkpoint Task 3**

Run: `git diff --check -- src-tauri/src/services/route_proxy_service.rs src-tauri/src/services/route_model_test_service.rs`

If commits are authorized:

```powershell
git add src-tauri/src/services/route_proxy_service.rs src-tauri/src/services/route_model_test_service.rs
git commit -m "fix: remove implicit codex routing fallbacks"
```

### Task 4: Expose Capabilities and Gate Accounts Actions

**Files:**
- Create: `src-tauri/src/commands/platform_commands.rs`
- Modify: `src-tauri/src/commands/mod.rs:1`
- Modify: `src-tauri/src/lib.rs:186`
- Modify: `src-tauri/src/web/handlers/mod.rs:35`
- Create: `src/lib/platformCapabilities.ts`
- Create: `src/lib/query/platformCapabilities.ts`
- Create: `src/components/platform/PlatformSupportBadge.tsx`
- Modify: `src/lib/api/types.ts:1`
- Modify: `src/lib/api/client.ts:37`
- Modify: `src/screens/AccountsScreen.tsx:78,1168,1761`
- Modify: `tests/AccountsScreen.test.tsx:1`
- Create: `tests/platformCapabilities.test.ts`

**Interfaces:**
- Consumes: `PlatformCapabilityService::list`.
- Produces: `list_platform_capabilities`, frontend `usePlatformCapabilities`, and capability-gated Accounts actions.

- [ ] **Step 1: Add backend command serialization tests**

Add a Web dispatcher test that invokes `list_platform_capabilities` and asserts seven rows, Hermes partial, and Hermes config writing unavailable.

- [ ] **Step 2: Implement and register the capability command**

```rust
#[tauri::command]
pub async fn list_platform_capabilities() -> Vec<PlatformCapability> {
    PlatformCapabilityService::list()
}
```

Register it in `commands/mod.rs`, Tauri `generate_handler!`, and Web `dispatch_command` using the same service result.

- [ ] **Step 3: Add frontend capability types and query**

Mirror the Rust DTOs in `src/lib/api/types.ts`, add `listPlatformCapabilities()` to `client.ts`, and create:

```ts
export function usePlatformCapabilities() {
  return useQuery({
    queryKey: ["platform-capabilities"],
    queryFn: listPlatformCapabilities,
    staleTime: Infinity,
  });
}
```

`platformCapabilities.ts` must export `findPlatformCapability`, `operationEnabled`, and `capabilityReason`. `operationEnabled` returns false only for `unavailable`; partial remains usable when the caller satisfies the returned constraints.

- [ ] **Step 4: Write frontend helper and Accounts behavior tests**

Test these exact behaviors:

```ts
it("marks Hermes partial and disables native config writing", () => {
  const rule = (
    availability: CapabilityAvailability,
    reason_code: string | null = null,
    credential_kinds: string[] = [],
  ): CapabilityRule => ({
    availability,
    reason_code,
    credential_kinds,
    requires_base_url: availability === "partial",
    requires_api_dialect: availability === "partial",
  });
  const hermesCapability: PlatformCapability = {
    platform: "hermes",
    display_name: "Hermes",
    support_level: "partial",
    operations: {
      route_credentials: rule("supported"),
      generic_api_routing: rule("partial", "capability.api_credentials_only", ["api"]),
      config_write: rule("unavailable", "capability.native_config_unavailable"),
      official_import: rule("unavailable", "capability.official_account_unavailable"),
      official_account_routing: rule("unavailable", "capability.official_account_unavailable"),
      deeplink_import: rule("unavailable", "capability.deeplink_unavailable"),
      official_quota: rule("unavailable", "capability.quota_unavailable"),
      model_test: rule("partial", "capability.api_credentials_only", ["api"]),
      terminal_launch: rule("supported"),
      session_resume: rule("supported"),
    },
  };
  const hermes = findPlatformCapability([hermesCapability], "hermes");
  expect(hermes?.support_level).toBe("partial");
  expect(operationEnabled(hermes!.operations.config_write)).toBe(false);
  expect(capabilityReason(hermes!.operations.config_write)).toContain("原生配置");
});
```

In `AccountsScreen.test.tsx`, reuse the same typed rule factory to mock a Hermes descriptor, render Hermes, and assert:

- the partial badge is visible;
- Write Config and official import are disabled with reasons;
- API credential creation remains available;
- quota refresh does not auto-run;
- model testing remains available only under the partial API constraint.

- [ ] **Step 5: Run frontend tests and verify failures**

Run: `pnpm test:run -- tests/platformCapabilities.test.ts tests/AccountsScreen.test.tsx`

Expected: FAIL because Accounts does not query capabilities or disable actions.

- [ ] **Step 6: Gate Accounts actions and show write errors**

Load the active descriptor once. Gate config write, official import mode, quota actions/auto-refresh, and model-test entry points by their operation rules. Keep unavailable controls visible, set `disabled`, and provide the mapped reason through nearby text and `title`/accessible description.

Add `configWriteError` state and mutation handlers:

```ts
const writeConfigsMutation = useMutation({
  mutationFn: () => writeRouteProxyConfigs(routeProxyQuery.data?.base_url ?? null, activePlatform),
  onMutate: () => setConfigWriteError(null),
  onSuccess: setConfigWriteOutcomes,
  onError: (error) => setConfigWriteError(formatApiError(error, "配置写入失败。")),
});
```

Render the error with `role="alert"`; do not auto-dismiss errors.

- [ ] **Step 7: Run backend and frontend focused tests**

Run: `cd src-tauri && cargo test list_platform_capabilities --lib`

Run: `pnpm test:run -- tests/platformCapabilities.test.ts tests/AccountsScreen.test.tsx`

Expected: PASS.

- [ ] **Step 8: Checkpoint Task 4**

Run: `git diff --check -- src-tauri/src/commands src-tauri/src/web/handlers/mod.rs src/lib src/components/platform src/screens/AccountsScreen.tsx tests`

If commits are authorized:

```powershell
git add src-tauri/src/commands/platform_commands.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs src-tauri/src/web/handlers/mod.rs src/lib/api/types.ts src/lib/api/client.ts src/lib/platformCapabilities.ts src/lib/query/platformCapabilities.ts src/components/platform/PlatformSupportBadge.tsx src/screens/AccountsScreen.tsx tests/platformCapabilities.test.ts tests/AccountsScreen.test.tsx
git commit -m "feat: gate account actions by platform capability"
```

### Task 5: Add Target Metadata and Snapshot Persistence

**Files:**
- Create: `src-tauri/migrations/202608010001_platform_capabilities_safe_writes.sql`
- Create: `src-tauri/src/models/config_snapshot.rs`
- Modify: `src-tauri/src/models/target_app.rs:1`
- Modify: `src-tauri/src/models/mod.rs:1`
- Create: `src-tauri/src/database/repositories/config_snapshot_repository.rs`
- Create: `src-tauri/src/database/repositories/target_state_repository.rs`
- Modify: `src-tauri/src/database/repositories/mod.rs:1`
- Modify: `src-tauri/src/database/repositories/target_repository.rs:10`
- Modify: `src-tauri/src/paths.rs:6,46`
- Modify: `src-tauri/src/database/test_support.rs:1`

**Interfaces:**
- Consumes: canonical platform IDs and existing `config_snapshots`/`target_app_states` tables.
- Produces: `ConfigSnapshotRecord`, secret-free `ConfigSnapshotSummary`, `ConfigWriteOutcome`, repository methods, target platform mapping, and `AppPaths.config_snapshots_dir`.

- [ ] **Step 1: Write migration and repository tests first**

Add tests asserting:

- `target_apps.platform` exists and defaults map `claude_code -> claude`, `grok -> grok`, and `hermes -> hermes`;
- `ensure_defaults` inserts a Grok target;
- a prepared snapshot can be marked succeeded and listed;
- public summaries do not contain `backup_path` or `metadata_json`;
- target state upsert replaces the prior status for one target.

- [ ] **Step 2: Run persistence tests and verify failures**

Run: `cd src-tauri && cargo test config_snapshot_repository --lib && cargo test migrations_create_foundation_tables --lib && cargo test target_repository --lib`

Expected: FAIL because the new columns, models, and repositories do not exist.

- [ ] **Step 3: Add the additive migration**

Use this schema change without editing old migrations:

```sql
ALTER TABLE target_apps ADD COLUMN platform TEXT;

UPDATE target_apps
SET platform = CASE key
  WHEN 'claude_code' THEN 'claude'
  WHEN 'claude_desktop' THEN 'claude'
  WHEN 'codex' THEN 'codex'
  WHEN 'gemini_cli' THEN 'gemini'
  WHEN 'grok' THEN 'grok'
  WHEN 'opencode' THEN 'opencode'
  WHEN 'openclaw' THEN 'openclaw'
  WHEN 'hermes' THEN 'hermes'
  ELSE NULL
END;

ALTER TABLE config_snapshots ADD COLUMN platform TEXT;
ALTER TABLE config_snapshots ADD COLUMN operation_group_id TEXT;
ALTER TABLE config_snapshots ADD COLUMN source_snapshot_id TEXT;
ALTER TABLE config_snapshots ADD COLUMN original_file_existed INTEGER NOT NULL DEFAULT 0 CHECK (original_file_existed IN (0, 1));
ALTER TABLE config_snapshots ADD COLUMN metadata_json TEXT NOT NULL DEFAULT '{}';
ALTER TABLE config_snapshots ADD COLUMN updated_at TEXT NOT NULL DEFAULT '';

UPDATE config_snapshots SET updated_at = created_at WHERE updated_at = '';

CREATE INDEX IF NOT EXISTS idx_config_snapshots_target_created
  ON config_snapshots(target_app_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_config_snapshots_group
  ON config_snapshots(operation_group_id);
```

- [ ] **Step 4: Add snapshot and outcome models**

`ConfigSnapshotRecord` mirrors every database column, including private `backup_path`. `ConfigSnapshotSummary` omits `backup_path` and `metadata_json`. `ConfigWriteOutcome` contains only:

```rust
pub struct ConfigWriteOutcome {
    pub operation_id: String,
    pub snapshot_id: Option<String>,
    pub target_app_id: Option<String>,
    pub target_key: String,
    pub platform: String,
    pub path: String,
    pub status: String,
    pub before_hash: Option<String>,
    pub after_hash: Option<String>,
    pub error_code: Option<String>,
}
```

Do not include `route_proxy_key`.

- [ ] **Step 5: Implement repositories and target mapping**

`ConfigSnapshotRepository` must provide:

```rust
pub async fn prepare(pool: &SqlitePool, input: NewConfigSnapshot) -> Result<ConfigSnapshotRecord, AppError>;
pub async fn mark_status(pool: &SqlitePool, id: &str, status: &str, after_hash: Option<&str>, error_code: Option<&str>) -> Result<(), AppError>;
pub async fn get(pool: &SqlitePool, id: &str) -> Result<ConfigSnapshotRecord, AppError>;
pub async fn list(pool: &SqlitePool, target_app_id: Option<&str>, limit: i64) -> Result<Vec<ConfigSnapshotSummary>, AppError>;
pub async fn latest_for_target(pool: &SqlitePool, target_app_id: &str) -> Result<Option<ConfigSnapshotSummary>, AppError>;
pub async fn count_for_target(pool: &SqlitePool, target_app_id: &str) -> Result<i64, AppError>;
pub async fn list_prepared_before(pool: &SqlitePool, cutoff: &str) -> Result<Vec<ConfigSnapshotRecord>, AppError>;
```

Add `TargetStateRepository::{get,record}` with an upsert on `target_app_id`. Add `TargetRepository::get_by_key`. Seed Grok and store platform for every default target without overwriting user display names.

- [ ] **Step 6: Add the private snapshot path**

Set `config_snapshots_dir` to `data_dir/backups/config-snapshots`, create it in `AppPaths::ensure`, and set Unix directory mode to `0700`. Backup files receive mode `0600` when created. Windows uses the inherited current-user profile ACL and never exposes backup bytes through APIs.

- [ ] **Step 7: Run persistence tests**

Run: `cd src-tauri && cargo test config_snapshot_repository --lib && cargo test target_state_repository --lib && cargo test database::test_support --lib && cargo test paths::tests --lib`

Expected: PASS.

- [ ] **Step 8: Checkpoint Task 5**

Run: `git diff --check -- src-tauri/migrations src-tauri/src/models src-tauri/src/database src-tauri/src/paths.rs`

If commits are authorized:

```powershell
git add src-tauri/migrations/202608010001_platform_capabilities_safe_writes.sql src-tauri/src/models/config_snapshot.rs src-tauri/src/models/target_app.rs src-tauri/src/models/mod.rs src-tauri/src/database/repositories/config_snapshot_repository.rs src-tauri/src/database/repositories/target_state_repository.rs src-tauri/src/database/repositories/target_repository.rs src-tauri/src/database/repositories/mod.rs src-tauri/src/database/test_support.rs src-tauri/src/paths.rs
git commit -m "feat: persist config snapshots and target platforms"
```

### Task 6: Replace Dead Adapters with a Production Route-Config Registry

**Files:**
- Rewrite: `src-tauri/src/adapters/mod.rs:1`
- Create: `src-tauri/src/adapters/route_config/mod.rs`
- Create: `src-tauri/src/adapters/route_config/codex.rs`
- Create: `src-tauri/src/adapters/route_config/json_agent.rs`
- Modify: `src-tauri/src/services/route_config_service.rs:159,434`
- Test: inline adapter tests and existing route-config service tests

**Interfaces:**
- Consumes: `PlatformId`, target keys from Task 5, existing merge/render behavior.
- Produces: `TargetAdapter`, `TargetAdapterRegistry`, `RouteConfigInput`, and `TargetInspection`.

- [ ] **Step 1: Write adapter registry and preservation tests**

Test exact registration:

```rust
#[test]
fn registry_contains_only_verified_native_config_adapters() {
    let registry = TargetAdapterRegistry::new();
    assert_eq!(registry.for_platform(PlatformId::Codex).unwrap().target_key(), "codex");
    assert_eq!(registry.for_platform(PlatformId::Claude).unwrap().target_key(), "claude_code");
    assert_eq!(registry.for_platform(PlatformId::Gemini).unwrap().target_key(), "gemini_cli");
    assert_eq!(registry.for_platform(PlatformId::Grok).unwrap().target_key(), "grok");
    assert!(registry.for_platform(PlatformId::Hermes).is_none());
    assert!(registry.by_target_key("claude_desktop").is_none());
}
```

Move/retain tests proving unmanaged TOML keys, JSON settings, and environment entries survive rendering. Add inspection tests for missing, unmanaged, managed, and invalid content.

- [ ] **Step 2: Run adapter tests and verify failures**

Run: `cd src-tauri && cargo test route_config:: --lib`

Expected: FAIL because production adapters do not exist.

- [ ] **Step 3: Define the adapter boundary**

```rust
pub struct RouteConfigInput {
    pub base_url: String,
    pub route_proxy_key: String,
}

pub struct TargetInspection {
    pub file_status: String,
    pub managed: bool,
    pub error_code: Option<String>,
}

pub trait TargetAdapter: Send + Sync {
    fn target_key(&self) -> &'static str;
    fn platform(&self) -> PlatformId;
    fn resolve_path(&self, home: &Path) -> PathBuf;
    fn render(&self, path: &Path, existing: Option<&[u8]>, input: &RouteConfigInput) -> Result<Vec<u8>, AppError>;
    fn inspect(&self, path: &Path, existing: Option<&[u8]>) -> TargetInspection;
}
```

The registry returns `Arc<dyn TargetAdapter>` and has no adapters for OpenCode, OpenClaw, Hermes, or Claude Desktop.

- [ ] **Step 4: Move the four verified renderers behind adapters**

Move Codex TOML merge logic into `codex.rs`. Move the shared Claude/Gemini/Grok JSON merge into `json_agent.rs`, parameterized by platform and target key. New files render UTF-8 bytes and reject invalid existing UTF-8/config with `config.generated_invalid` or the existing invalid-config error.

Inspection rules:

- missing bytes -> `missing`;
- parse failure -> `invalid`;
- Codex `model_provider = "ai-switch"` plus `model_providers.ai-switch` -> `managed`;
- JSON `aiSwitch.routeProxy.enabled == true` -> `managed`;
- valid config without those markers -> `unmanaged`.

- [ ] **Step 5: Delegate route-config planning to the registry**

Keep the current direct write path temporarily, but replace `route_config_target` and render switches with adapter lookup and `adapter.render`. Require `PlatformOperation::ConfigWrite` before generating or inserting a proxy key.

- [ ] **Step 6: Run adapter and existing route-config tests**

Run: `cd src-tauri && cargo test route_config --lib`

Expected: PASS with identical managed output for Codex/Claude/Gemini/Grok and explicit unsupported errors for the other three platforms.

- [ ] **Step 7: Checkpoint Task 6**

Run: `git diff --check -- src-tauri/src/adapters src-tauri/src/services/route_config_service.rs`

If commits are authorized:

```powershell
git add src-tauri/src/adapters src-tauri/src/services/route_config_service.rs
git commit -m "refactor: register verified config adapters"
```

### Task 7: Build Conflict-Aware Atomic File Primitives

**Files:**
- Modify: `src-tauri/src/config_writer/mod.rs:18`
- Create: `src-tauri/src/config_writer/platform.rs`
- Modify: `src-tauri/Cargo.toml` only if an additional existing `windows-sys` feature is required
- Test: inline config-writer tests

**Interfaces:**
- Consumes: filesystem paths and expected SHA-256 state.
- Produces: `FileState`, `ConfigWriter::{inspect,write_atomic_if_unchanged,remove_if_hash_matches,write_private_backup}`.

- [ ] **Step 1: Write low-level safety tests**

Add tests for:

```rust
#[tokio::test]
async fn write_refuses_a_changed_target() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.json");
    tokio::fs::write(&path, b"old").await.unwrap();
    let expected = ConfigWriter::inspect(&path).await.unwrap();
    tokio::fs::write(&path, b"external").await.unwrap();
    let error = ConfigWriter::write_atomic_if_unchanged(&path, b"new", &expected).await.unwrap_err();
    assert!(matches!(error, AppError::Validation { code: "config.concurrent_modification", .. }));
    assert_eq!(tokio::fs::read(&path).await.unwrap(), b"external");
}

#[tokio::test]
async fn remove_requires_the_recorded_after_hash() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("created.json");
    tokio::fs::write(&path, b"created").await.unwrap();
    let created_hash = ConfigWriter::inspect(&path).await.unwrap().hash.unwrap();
    ConfigWriter::remove_if_hash_matches(&path, &created_hash).await.unwrap();
    assert!(!tokio::fs::try_exists(&path).await.unwrap());

    tokio::fs::write(&path, b"changed").await.unwrap();
    let error = ConfigWriter::remove_if_hash_matches(&path, &created_hash).await.unwrap_err();
    assert!(matches!(error, AppError::Validation { code: "config.rollback_conflict", .. }));
    assert_eq!(tokio::fs::read(&path).await.unwrap(), b"changed");
}
```

Add Unix mode preservation, target symlink refusal, parent-directory symlink refusal, temp cleanup, and Windows existing/new target tests under the relevant `cfg` attributes.

- [ ] **Step 2: Run config-writer tests and verify failures**

Run: `cd src-tauri && cargo test config_writer --lib`

Expected: FAIL because expected-state and safe-delete APIs do not exist.

- [ ] **Step 3: Implement byte-oriented file state and link checks**

```rust
pub struct FileState {
    pub existed: bool,
    pub bytes: Option<Vec<u8>>,
    pub hash: Option<String>,
    pub permissions: Option<std::fs::Permissions>,
}
```

`inspect` uses `symlink_metadata` on the target and immediate parent. Reject Unix symlinks and Windows paths with `FILE_ATTRIBUTE_REPARSE_POINT` using `config.path_unsafe`. Missing targets return `existed: false` without creating directories.

- [ ] **Step 4: Implement conditional atomic replacement**

`write_atomic_if_unchanged` re-inspects existence/hash immediately before replacement. It writes a UUID temp file in the same directory, flushes and syncs it, copies existing standard permissions or uses Unix `0600` for a new config, then commits:

- existing Windows target: `ReplaceFileW`;
- new Windows target: `MoveFileExW(MOVEFILE_WRITE_THROUGH)` without replacement;
- Unix: `rename`, followed by parent-directory sync.

Always remove the temp file after a failed commit. Read the target after commit and require its SHA-256 to equal the expected new hash; otherwise return `config.verify_failed`.

Retain a compatibility `write_atomic(path: &Path, content: &str)` wrapper through Task 8. It calls `inspect` followed by `write_atomic_if_unchanged`; Task 9 removes the wrapper after all route config callers use the coordinator.

- [ ] **Step 5: Implement guarded delete and private backup write**

`remove_if_hash_matches` requires current existence and hash equality before `remove_file`; mismatch returns `config.rollback_conflict`. `write_private_backup` creates/syncs exact bytes and applies Unix `0600`.

- [ ] **Step 6: Run config-writer tests on the current OS**

Run: `cd src-tauri && cargo test config_writer --lib`

Expected: PASS; OS-specific tests compile only on their target.

- [ ] **Step 7: Checkpoint Task 7**

Run: `git diff --check -- src-tauri/src/config_writer src-tauri/Cargo.toml`

If commits are authorized:

```powershell
git add src-tauri/src/config_writer src-tauri/Cargo.toml
git commit -m "feat: add conflict-aware atomic config writes"
```

### Task 8: Implement Snapshot-Backed Single Writes and Rollback

**Files:**
- Create: `src-tauri/src/services/config_write_service.rs`
- Modify: `src-tauri/src/services/mod.rs:1`
- Modify: `src-tauri/src/app_state.rs:11`
- Modify: `src-tauri/src/lib.rs:139`
- Modify: `src-tauri/src/server.rs:29`
- Modify: `src-tauri/src/web/handlers/mod.rs:500`
- Modify: `src-tauri/src/services/route_proxy_https_service.rs:1203`
- Test: inline coordinator tests

**Interfaces:**
- Consumes: adapter mutation plans, safe file primitives, snapshot/target-state repositories.
- Produces: `ConfigWriteRuntimeState`, `ConfigWriteCoordinator::{write_one,rollback}`.

- [ ] **Step 1: Write coordinator transaction tests**

Cover these exact cases:

- existing file: backup bytes equal original, snapshot succeeds, target has rendered bytes;
- new file: no backup path, `original_file_existed = 0`;
- snapshot insert failure: target unchanged and backup cleaned;
- external change before commit: snapshot becomes `conflict`, target external bytes remain;
- rollback existing file: original bytes restored and a rollback snapshot is recorded;
- rollback new file: file deleted only on matching after hash;
- rollback changed file: `config.rollback_conflict`, no deletion/write.

- [ ] **Step 2: Run coordinator tests and verify failures**

Run: `cd src-tauri && cargo test config_write_service --lib`

Expected: FAIL because the coordinator and runtime state do not exist.

- [ ] **Step 3: Add shared per-path runtime locks**

```rust
#[derive(Clone, Default)]
pub struct ConfigWriteRuntimeState {
    locks: Arc<tokio::sync::Mutex<HashMap<PathBuf, Arc<tokio::sync::Mutex<()>>>>>,
}
```

Return one `Arc<Mutex<()>>` per normalized absolute path. Add `config_writes: ConfigWriteRuntimeState` to every `AppState` constructor and test fixture.

- [ ] **Step 4: Implement single-write preparation and commit**

Expose `prepare_one` and `commit_prepared` as `pub(crate)` methods for deterministic service tests; production `write_one` holds the path lock and composes both methods. `ConfigWriteCoordinator::write_one` must:

1. resolve target row by adapter target key;
2. lock path;
3. inspect original bytes;
4. render and validate replacement bytes;
5. generate `operation_id` and `snapshot_id` UUIDs;
6. write exact backup bytes when present;
7. insert a `prepared` snapshot;
8. call `write_atomic_if_unchanged`;
9. mark snapshot `succeeded`, `failed`, or `conflict`;
10. upsert `target_app_states`;
11. return secret-free `ConfigWriteOutcome`.

Use metadata JSON containing only adapter key and operation kind.

- [ ] **Step 5: Implement guarded rollback**

`rollback(snapshot_id)` loads only a `succeeded` write snapshot, locks its path, requires the current hash to match its `after_hash`, records a new prepared rollback snapshot, and restores exact backup bytes or calls `remove_if_hash_matches` for a file created by AI Switch. Return a new `ConfigWriteOutcome`; never mutate the selected historical row.

- [ ] **Step 6: Reconcile interrupted prepared rows read-only**

Add `reconcile_prepared()` that selects only `prepared` rows older than five minutes, compares current existence/hash, and marks them `succeeded`, `failed`, or `conflict` without writing any target. Call it before target status/snapshot listing, not during app startup. The five-minute cutoff prevents a status query from reconciling an active small-file write.

- [ ] **Step 7: Run coordinator and AppState-dependent tests**

Run: `cd src-tauri && cargo test config_write_service --lib && cargo test web::handlers --lib && cargo test route_proxy_https_service --lib`

Expected: PASS.

- [ ] **Step 8: Checkpoint Task 8**

Run: `git diff --check -- src-tauri/src/services/config_write_service.rs src-tauri/src/app_state.rs src-tauri/src/lib.rs src-tauri/src/server.rs src-tauri/src/web/handlers/mod.rs src-tauri/src/services/route_proxy_https_service.rs`

If commits are authorized:

```powershell
git add src-tauri/src/services/config_write_service.rs src-tauri/src/services/mod.rs src-tauri/src/app_state.rs src-tauri/src/lib.rs src-tauri/src/server.rs src-tauri/src/web/handlers/mod.rs src-tauri/src/services/route_proxy_https_service.rs
git commit -m "feat: snapshot config writes and guarded rollback"
```

### Task 9: Integrate Grouped Route-Config Writes and Hermes Protection

**Files:**
- Modify: `src-tauri/src/services/config_write_service.rs`
- Modify: `src-tauri/src/services/route_config_service.rs:15,39,223,245`
- Modify: `src-tauri/src/commands/route_proxy_commands.rs:30`
- Modify: `src-tauri/src/services/route_proxy_https_service.rs:465`
- Modify: `src-tauri/src/models/route_proxy_https.rs:3,71`
- Modify: `src/lib/api/types.ts:323`
- Modify: `tests/AccountsScreen.test.tsx`
- Test: route-config and coordinator inline tests

**Interfaces:**
- Consumes: coordinator single-write/rollback and target adapters.
- Produces: `ConfigWriteCoordinator::write_group`; route config and HTTPS rewrites use snapshots; responses no longer contain proxy keys.

- [ ] **Step 1: Write grouped failure and Hermes sentinel tests**

Add a group test where target A commits, target B fails, and target A restores only when its current hash equals the group after hash. Add a second test that externally changes target A before rollback and expects a conflict without overwrite.

Add the Hermes sentinel test:

```rust
#[tokio::test]
async fn every_route_config_entry_point_leaves_hermes_config_untouched() {
    let fixture = tempdir().unwrap();
    let home = fixture.path().join("home");
    let paths = AppPaths::from_data_dir(fixture.path().join("app-data"));
    paths.ensure().await.unwrap();
    let pool = create_memory_pool().await.unwrap();
    run_migrations(&pool).await.unwrap();
    let runtime = ConfigWriteRuntimeState::default();
    let hermes = home.join(".hermes").join("config.yaml");
    tokio::fs::create_dir_all(hermes.parent().unwrap()).await.unwrap();
    tokio::fs::write(&hermes, b"model: sentinel\n").await.unwrap();
    let before = ConfigWriter::inspect(&hermes).await.unwrap();

    let error = RouteConfigService::write_configs_for_home(
        &paths,
        &pool,
        &runtime,
        "http://127.0.0.1:43111",
        "hermes",
        &home,
    )
    .await
    .unwrap_err();
    assert!(matches!(error, AppError::Validation { code: "capability.unavailable", .. }));

    RouteProxyKeyRepository::ensure_platform_key(&pool, "hermes", "sk-ai-switch-hermes")
        .await
        .unwrap();
    let outcomes = RouteConfigService::write_existing_configs_for_home(
        &paths,
        &pool,
        &runtime,
        "http://127.0.0.1:43111",
        &home,
    )
    .await
    .unwrap();
    assert!(outcomes.iter().any(|item| item.platform == "hermes" && item.status == "skipped"));

    let after = ConfigWriter::inspect(&hermes).await.unwrap();
    assert_eq!(after.hash, before.hash);
    assert_eq!(after.bytes, before.bytes);
}
```

- [ ] **Step 2: Run grouped and Hermes tests and verify failures**

Run: `cd src-tauri && cargo test write_group --lib && cargo test hermes_config --lib`

Expected: FAIL because route config still writes directly and grouped snapshots do not exist.

- [ ] **Step 3: Add deterministic grouped locking and preflight**

Resolve all paths first, sort/deduplicate them, and acquire owned path guards in sorted order. Preflight every adapter render, backup, and prepared snapshot before committing the first target. Use one operation group UUID for all outcomes.

- [ ] **Step 4: Commit sequentially and roll back safely**

Commit prepared mutations in input order. On failure, walk committed mutations in reverse and restore only when current hash equals the recorded after hash. Record per-target `succeeded`, `failed`, or `conflict`; return one filesystem error containing the operation ID and target statuses, without config bytes.

- [ ] **Step 5: Replace direct route-config writes**

Update service signatures to receive `&ConfigWriteRuntimeState`. `write_configs` checks capability and adapter before `ensure_platform_key`, then delegates one request to `write_group`. If the write fails and the key was created by this attempt, retain the existing `delete_if_matches` database cleanup.

`write_existing_configs` builds requests only for keys with registered adapters. Known unsupported or unknown legacy keys produce `skipped` outcomes with `config.adapter_unavailable`; they do not fail supported HTTPS rewrites and do not resolve a config path.

Delete direct `ConfigWriter::write_atomic`, `rollback_route_config_plans`, and raw `remove_file` usage from `route_config_service.rs`.

- [ ] **Step 6: Remove route proxy keys from outcomes**

Use `ConfigWriteOutcome` from Task 5 for route config and HTTPS outcomes. Update Rust/TypeScript models and Accounts fixtures. Accounts displays target, path, status, snapshot ID, and hashes; it never renders the route proxy key.

- [ ] **Step 7: Run route-config, HTTPS, and Hermes tests**

Run: `cd src-tauri && cargo test route_config_service --lib && cargo test config_write_service --lib && cargo test route_proxy_https_service --lib`

Expected: PASS, including the sentinel assertion.

- [ ] **Step 8: Run Accounts tests after response-shape change**

Run: `pnpm test:run -- tests/AccountsScreen.test.tsx`

Expected: PASS.

- [ ] **Step 9: Checkpoint Task 9**

Run: `git diff --check -- src-tauri/src/services/config_write_service.rs src-tauri/src/services/route_config_service.rs src-tauri/src/commands/route_proxy_commands.rs src-tauri/src/services/route_proxy_https_service.rs src-tauri/src/models/route_proxy_https.rs src/lib/api/types.ts tests/AccountsScreen.test.tsx`

If commits are authorized:

```powershell
git add src-tauri/src/services/config_write_service.rs src-tauri/src/services/route_config_service.rs src-tauri/src/commands/route_proxy_commands.rs src-tauri/src/services/route_proxy_https_service.rs src-tauri/src/models/route_proxy_https.rs src/lib/api/types.ts tests/AccountsScreen.test.tsx
git commit -m "feat: route config writes through safe transactions"
```

### Task 10: Expose Target Status, Snapshots, and Rollback

**Files:**
- Modify: `src-tauri/src/models/target_app.rs:1`
- Modify: `src-tauri/src/services/target_service.rs:6`
- Modify: `src-tauri/src/commands/target_commands.rs:8`
- Modify: `src-tauri/src/lib.rs:51,225`
- Modify: `src-tauri/src/web/handlers/mod.rs:35`
- Modify: `src/lib/api/types.ts:352`
- Modify: `src/lib/api/client.ts:80`
- Test: target service, command, and Web dispatcher tests

**Interfaces:**
- Consumes: target repository, adapter registry, snapshot repository, coordinator rollback.
- Produces: `list_target_config_statuses`, `list_config_snapshots`, and `rollback_config_snapshot` across both transports.

- [ ] **Step 1: Write target status and command tests**

Create temporary homes containing:

- managed Codex config;
- invalid Claude JSON;
- missing Gemini config;
- unsupported Hermes target.

Assert statuses `managed`, `invalid`, `missing`, and `adapter_unavailable`, with Hermes `config_path: None`. Add command tests proving snapshot summaries omit backup paths and rollback returns a new operation/snapshot ID.

- [ ] **Step 2: Run target tests and verify failures**

Run: `cd src-tauri && cargo test target_service --lib && cargo test target_commands --lib`

Expected: FAIL because only `list_target_apps` exists.

- [ ] **Step 3: Add target config status DTOs and inspection service**

```rust
pub struct TargetConfigStatus {
    pub target: TargetApp,
    pub support_level: Option<String>,
    pub adapter_available: bool,
    pub config_path: Option<String>,
    pub file_status: String,
    pub last_write_status: Option<String>,
    pub last_error_code: Option<String>,
    pub last_written_at: Option<String>,
    pub snapshot_count: i64,
    pub latest_snapshot: Option<ConfigSnapshotSummary>,
}
```

`TargetService::list_config_statuses_for_home` ensures defaults, reconciles prepared rows read-only, parses each target platform, resolves adapter by target key, inspects only registered adapter paths, and joins target state/snapshot metadata.

If a legacy target has a missing or unrecognized platform value, return `support_level: None`, `adapter_available: false`, `config_path: None`, and `file_status: "unrecognized"`; do not fail the whole list.

- [ ] **Step 4: Add commands and shared transport handlers**

```rust
#[tauri::command]
pub async fn list_target_config_statuses(state: State<'_, AppState>) -> Result<Vec<TargetConfigStatus>, ApiError>;

#[tauri::command]
pub async fn list_config_snapshots(state: State<'_, AppState>, target_app_id: Option<String>, limit: Option<i64>) -> Result<Vec<ConfigSnapshotSummary>, ApiError>;

#[tauri::command]
pub async fn rollback_config_snapshot(state: State<'_, AppState>, id: String) -> Result<ConfigWriteOutcome, ApiError>;
```

Clamp list limits to `1..=200`, default `50`. Register all three in Tauri and Web using the same services.

- [ ] **Step 5: Add frontend types and client methods**

Add `TargetConfigStatus`, `ConfigSnapshotSummary`, and the updated `ConfigWriteOutcome` types. Add camelCase command args in the client: `{ targetAppId, limit }` and `{ id }`; Web handlers read the same names.

- [ ] **Step 6: Run backend transport tests**

Run: `cd src-tauri && cargo test target_service --lib && cargo test web::handlers --lib`

Expected: PASS.

- [ ] **Step 7: Checkpoint Task 10**

Run: `git diff --check -- src-tauri/src/models/target_app.rs src-tauri/src/services/target_service.rs src-tauri/src/commands/target_commands.rs src-tauri/src/lib.rs src-tauri/src/web/handlers/mod.rs src/lib/api/types.ts src/lib/api/client.ts`

If commits are authorized:

```powershell
git add src-tauri/src/models/target_app.rs src-tauri/src/services/target_service.rs src-tauri/src/commands/target_commands.rs src-tauri/src/lib.rs src-tauri/src/web/handlers/mod.rs src/lib/api/types.ts src/lib/api/client.ts
git commit -m "feat: expose config status and rollback history"
```

### Task 11: Replace Placeholder Screens with Real Config Data

**Files:**
- Modify: `src/screens/TargetsScreen.tsx:4`
- Modify: `src/screens/DashboardScreen.tsx:1`
- Modify: `src/screens/OperationLogScreen.tsx:1`
- Modify: `src/screens/ProvidersScreen.tsx:1`
- Modify: `src/screens/AccountsScreen.tsx:1761`
- Create: `tests/TargetsScreen.test.tsx`
- Create: `tests/DashboardScreen.test.tsx`
- Create: `tests/OperationLogScreen.test.tsx`
- Modify: `tests/AccountsScreen.test.tsx`

**Interfaces:**
- Consumes: capability, target-status, snapshot, and rollback client APIs.
- Produces: data-backed Targets/Dashboard/Operation Log and accurate deferred-feature copy.

- [ ] **Step 1: Write screen tests before implementation**

Targets tests assert:

- Codex shows its real path, managed state, latest write, and rollback action;
- Hermes shows partial support and adapter unavailable with no path/write/rollback action;
- rollback mutation invalidates both target and snapshot queries;
- rollback conflict is shown with `role="alert"`.

Dashboard tests assert there is no unconditional `Ready`, and counts come from mocked capabilities/statuses/snapshots. Operation Log tests assert only returned config write/rollback rows render; no import-event claim appears.

- [ ] **Step 2: Run screen tests and verify failures**

Run: `pnpm test:run -- tests/TargetsScreen.test.tsx tests/DashboardScreen.test.tsx tests/OperationLogScreen.test.tsx`

Expected: FAIL because the screens are static/list-only.

- [ ] **Step 3: Implement Targets as the config status center**

Query `listTargetConfigStatuses`. Render target/platform, adapter badge, path, file status, latest outcome, and snapshot count. Expand a target to query `listConfigSnapshots(target.id, 50)`. Enable rollback only for successful write snapshots whose target still has an adapter; call `rollbackConfigSnapshot` and invalidate:

```ts
await Promise.all([
  queryClient.invalidateQueries({ queryKey: ["target-config-statuses"] }),
  queryClient.invalidateQueries({ queryKey: ["config-snapshots"] }),
]);
```

- [ ] **Step 4: Implement data-backed Dashboard and Operation Log**

Dashboard queries capabilities, target statuses, and the latest 50 snapshots. Calculate native adapter count, partial platform count, successful config operations, and failed/conflict operations. Omit a card while its source query is unavailable.

Operation Log queries snapshots with no target filter and labels itself "Config Operations" for phase A. Render operation, target/path, status, time, and hashes. Do not render backup paths or config metadata.

- [ ] **Step 5: Correct deferred Providers copy and Accounts outcomes**

Providers states that the standalone provider catalog is not implemented and that current agent tabs manage route credentials only. Accounts result rows show operation/snapshot IDs, target path, status, and hashes; remove any key-like output.

- [ ] **Step 6: Run all affected frontend tests**

Run: `pnpm test:run -- tests/AccountsScreen.test.tsx tests/TargetsScreen.test.tsx tests/DashboardScreen.test.tsx tests/OperationLogScreen.test.tsx`

Expected: PASS.

- [ ] **Step 7: Checkpoint Task 11**

Run: `git diff --check -- src/screens tests`

If commits are authorized:

```powershell
git add src/screens/AccountsScreen.tsx src/screens/TargetsScreen.tsx src/screens/DashboardScreen.tsx src/screens/OperationLogScreen.tsx src/screens/ProvidersScreen.tsx tests/AccountsScreen.test.tsx tests/TargetsScreen.test.tsx tests/DashboardScreen.test.tsx tests/OperationLogScreen.test.tsx
git commit -m "feat: show real platform and config status"
```

### Task 12: Complete Web Parity, Structured Errors, Documentation, and Validation

**Files:**
- Create: `src/lib/api/errors.ts`
- Create: `src/lib/api/commandSupport.ts`
- Modify: `src/lib/transport/tauri-transport.ts:32`
- Modify: `src/lib/transport/web-transport.ts:52`
- Modify: `src-tauri/src/web/handlers/mod.rs:35,383`
- Modify: `src-tauri/src/web/router.rs:44,108`
- Modify: `tests/transport/transport.test.ts`
- Create: `tests/transport/command-contract.test.ts`
- Modify: `README.md:1`
- Test: Web handler tests and full project suites

**Interfaces:**
- Consumes: all command names and `ApiError` fields.
- Produces: `ApiClientError`, structured Web errors, complete client-command parity except one explicit desktop-only command, and public capability documentation.

- [ ] **Step 1: Write structured error transport tests**

```ts
it("preserves Web API error codes", async () => {
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response(JSON.stringify({
    code: "capability.unavailable",
    message: "Not supported",
    details: "hermes:config_write",
    recoverable: true,
    operation_id: null,
  }), { status: 400, headers: { "Content-Type": "application/json" } })));

  await expect(new WebTransport("http://127.0.0.1:3090").call("write_route_proxy_configs"))
    .rejects.toMatchObject({ code: "capability.unavailable", recoverable: true });
});
```

Add the equivalent Tauri rejection-object test.

- [ ] **Step 2: Implement `ApiClientError` normalization**

```ts
export class ApiClientError extends Error {
  constructor(
    message: string,
    public readonly code: string,
    public readonly details: string | null,
    public readonly recoverable: boolean,
    public readonly operationId: string | null,
  ) { super(message); }
}
```

Both transports catch/parse object or string failures and throw this class. Web no longer discards `code`, `details`, or `operation_id`.

- [ ] **Step 3: Make Web dispatch return `ApiError`**

Change `dispatch_command` to `Result<Value, ApiError>`. Convert `AppError` with `ApiError::from`; convert missing/invalid arguments to `AppError::Validation` with stable `web.argument_missing` or `web.argument_invalid` codes. `api_command` serializes the returned `ApiError` directly instead of wrapping it as `web.error`.

- [ ] **Step 4: Add the currently missing Web handlers**

Implement shared-service arms for this exact list:

```text
copy_route_credential
create_batch
create_official_account
get_official_account
import_example_json
list_batch_groups
list_target_apps
refresh_route_credential_quota
refresh_route_credentials_quota
update_official_account
```

Keep `open_route_proxy_https_certificate_dir` desktop-only. Record it in `desktopOnlyCommands` and retain the existing `isDesktop()` UI gate.

- [ ] **Step 5: Add the command contract test**

Read `src/lib/api/client.ts`, `src-tauri/src/lib.rs`, and `src-tauri/src/web/handlers/mod.rs`. Extract literal client invokes, Tauri handler identifiers, and Web match arms. Assert every client command exists in Tauri and every non-desktop-only client command exists in Web. Permit Web-only `health` and `get_route_credential`.

- [ ] **Step 6: Run transport and Web tests**

Run: `pnpm test:run -- tests/transport/transport.test.ts tests/transport/command-contract.test.ts`

Run: `cd src-tauri && cargo test web::handlers --lib && cargo test web::router --lib`

Expected: PASS.

- [ ] **Step 7: Document the truthful support matrix**

Add a README section stating:

- native config writing: Codex, Claude, Gemini, Grok;
- partial generic API/terminal/session support: OpenCode, OpenClaw, Hermes;
- no native config write/import/quota claims for the partial platforms;
- safe writes create snapshots, detect conflicts, and support guarded rollback;
- AI Switch does not touch Hermes `config.yaml` in phase A.

- [ ] **Step 8: Run the complete validation sequence**

Run in this order:

```powershell
cd src-tauri
cargo test
cargo check
cd ..
pnpm test:run
pnpm typecheck
pnpm build
git diff --check
```

Expected: every command exits `0`. If a pre-existing unrelated failure appears, record it without modifying unrelated code.

- [ ] **Step 9: Verify workspace scope**

Run: `git status --short`

Confirm the implementation did not stage or overwrite pre-existing `src-tauri/Cargo.lock`, `tauri-dev.err`, or `tauri-dev.log.err` unless the user explicitly requested those files.

- [ ] **Step 10: Checkpoint Task 12**

If commits are authorized:

```powershell
git add src/lib/api/errors.ts src/lib/api/commandSupport.ts src/lib/transport/tauri-transport.ts src/lib/transport/web-transport.ts src-tauri/src/web/handlers/mod.rs src-tauri/src/web/router.rs tests/transport/transport.test.ts tests/transport/command-contract.test.ts README.md
git commit -m "fix: align web transport with desktop capabilities"
```

## Final Review Checklist

- [ ] Every canonical platform and capability rule has backend unit coverage.
- [ ] No service-local platform normalizer remains for agent platform identity.
- [ ] No `_ => codex`, `_ => OpenAI official URL`, or partial-platform API-dialect default remains.
- [ ] Only Codex, Claude Code, Gemini CLI, and Grok adapters are registered.
- [ ] Every config mutation creates a prepared snapshot before disk mutation.
- [ ] New-file rollback deletes only on matching recorded hash.
- [ ] Hermes sentinel bytes remain unchanged across every config-write entry point.
- [ ] Accounts, Targets, Dashboard, and Operation Log display only backend-derived support/status.
- [ ] Web errors retain the same code/details shape as Tauri errors.
- [ ] Client/Tauri/Web command contract test passes with only the declared desktop-only exception.
- [ ] README matches the shipped matrix.
- [ ] Full Rust/frontend tests, checks, and build pass.
