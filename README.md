# ai-switch

AI Switch is a Tauri-based desktop foundation for AI provider and official account switching.

Phase A includes:

- Tauri 2 + React + TypeScript app shell
- Rust backend with typed Tauri commands
- SQLite foundation schema
- Batch-first provider and account grouping
- Example JSON import into a named batch
- Settings stored in `~/.ai-switch/settings.json`
- Atomic config writer primitives
- Extension interfaces for target adapters, importers, quota providers, and secret storage

## Development

Install dependencies:

```powershell
corepack enable
pnpm install
```

Run frontend checks:

```powershell
pnpm typecheck
pnpm test:run
```

Run Rust checks:

```powershell
pnpm rust:check
pnpm rust:test
```

Run the desktop app in development mode:

```powershell
pnpm tauri:dev
```

## Clean-Room Boundary

This project may study public behavior, public documentation, and public file formats from related tools. It must not copy or translate non-commercial source code from `cockpit-tools`.

## Provider Switching B1

Provider switching B1 writes sandbox target configs only. It does not write real Claude, Codex, Gemini, OpenCode, OpenClaw, or Hermes configuration files.

Sandbox output path:

```text
~/.ai-switch/targets/<target_key>/provider.json
```

Verification flow:

1. Import or create a provider.
2. Open `Providers`.
3. Select a target such as `Codex`.
4. Click `Switch in sandbox`.
5. Open `Targets`.
6. Confirm the target shows the active provider, write status, and sandbox output path.

## Provider Switching B2.1: Codex Real Mode

B2.1 adds explicit real provider switching for Codex only. Sandbox switching remains available for all supported targets.

Codex real mode writes:

```text
<CODEX_HOME>/config.toml
```

If `CODEX_HOME` is not set, the app uses:

```text
~/.codex/config.toml
```

The Codex config contains provider metadata such as `model_provider`, `base_url`, `wire_api`, and `env_key`. It does not store raw API keys.

Safe smoke test:

1. Set `CODEX_HOME` to a temporary directory.
2. Start the app with `pnpm tauri:dev`.
3. Import or create a provider with `base_url`.
4. Open `Providers`.
5. Select `Codex`.
6. Click `Switch Codex config`.
7. Verify `<CODEX_HOME>/config.toml` contains `model_provider` and `[model_providers.ai_switch_<id>]`.
8. Verify your real `~/.codex/config.toml` was not modified when using temporary `CODEX_HOME`.
