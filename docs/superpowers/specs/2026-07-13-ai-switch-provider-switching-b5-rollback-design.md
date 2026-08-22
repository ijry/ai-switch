# AI Switch Provider Switching B5 Rollback Design

## Context

B2.1, B2.2, and B2.3 added real provider switching for Codex, OpenCode, and Gemini CLI. Those writes create `config_snapshots` rows with before/after hashes; B5 makes successful real-write snapshots restorable by requiring backup metadata.

B5 adds user-facing rollback for real provider switch writes. Rollback must restore the target config file to the state it had immediately before a successful real switch, or remove the file when the original file did not exist.

## Goals

- Save rollback backup metadata for every successful real provider switch.
- Keep sandbox switching unchanged and non-restorable in B5.
- Add a backend rollback command that restores from a specific real-write snapshot.
- Add Targets UI affordance for the latest restorable snapshot.
- Record rollback attempts as new `config_snapshots` rows.
- Update target switch state after rollback so the UI no longer claims the rolled-back provider is active.

## Non-Goals

- No rollback for sandbox writes.
- No rollback for failed snapshots.
- No full snapshot history browser.
- No provider/account inference from restored third-party config files.
- No schema migration; B5 uses existing `backup_path`, `before_hash`, and `after_hash`.

## Backup Model

Real switch writes call a backup-aware atomic writer.

If the target file existed before the real switch:

- `before_hash` stores the hash of the original bytes.
- `backup_path` points to a copy under `AppPaths.backups_dir/config/<target_key>/`.
- Rollback verifies the backup hash before restoring.

If the target file did not exist before the real switch:

- `before_hash` is `NULL`.
- `backup_path` points to a marker file under `AppPaths.backups_dir/config/<target_key>/`.
- Rollback removes the target file if it exists.

Sandbox writes continue to use normal atomic writes and leave `backup_path = NULL`.

## Rollback Command

`rollback_config_snapshot(snapshot_id)`:

1. Loads the snapshot.
2. Requires `operation = "switch_provider:real"` and `status = "written"`.
3. Requires a `target_app_id` and a `backup_path`.
4. Requires the backup path to stay under `AppPaths.backups_dir`.
5. Restores backup bytes when `before_hash` exists, or deletes the target file when `before_hash` is absent.
6. Inserts a `rollback_config:real` snapshot with the rollback result.
7. Updates target state to `rolled_back` and clears the active provider.

Stable validation/filesystem error codes:

- `validation.rollback_snapshot_not_supported`
- `validation.rollback_snapshot_missing_target`
- `validation.rollback_backup_missing`
- `filesystem.rollback_backup_outside_app`
- `filesystem.rollback_backup_hash_mismatch`

## UI

Targets cards show rollback only when the latest snapshot is a restorable real-write snapshot. The action calls `rollback_config_snapshot(last_snapshot_id)` and refreshes target statuses on success.

The UI labels rollback conservatively: it restores the prior config file state and does not promise provider-level undo semantics.

## Completion Criteria

- Real Codex/Gemini CLI/OpenCode writes create restorable backup paths.
- Rollback restores previous file content for existing-file writes.
- Rollback deletes the target file for newly-created real config files.
- Rollback records a new snapshot and marks target state `rolled_back`.
- Targets UI exposes and tests the rollback action.
- Rust and frontend tests pass.
