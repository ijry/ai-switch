# AI Switch Provider Switching B5 Rollback Implementation Plan

**Goal:** Add safe rollback for successful real provider switch snapshots.

**Architecture:** Extend `ConfigWriter` with backup-aware atomic writes, store backup paths in `config_snapshots`, add a rollback service/command on top of existing snapshots, and expose the latest restorable real snapshot from the Targets screen.

## Guardrails

- B5 must not add a schema migration.
- B5 must not expose sandbox rollback.
- B5 must not restore from failed snapshots.
- B5 must keep backup files under `AppPaths.backups_dir`.
- B5 must clear active provider state after rollback because restored config content is not parsed back into an app provider identity.

## Steps

- [x] Extend `WriteOutcome` and `ConfigWriter` with backup-aware real writes.
- [x] Save `backup_path` for real Codex/OpenCode switch snapshots.
- [x] Add repository helpers to load snapshots by id.
- [x] Add target-state rollback update helper.
- [x] Add rollback service and Tauri command.
- [x] Extend target switch status with rollback metadata.
- [x] Add API client/types and Targets UI rollback action.
- [x] Add Rust tests for restore-file and remove-new-file rollback paths.
- [x] Add frontend tests for visible rollback and command invocation.
- [x] Run `cargo fmt`, `pnpm typecheck`, `pnpm test:run`, `pnpm rust:check`, and `pnpm rust:test`.
