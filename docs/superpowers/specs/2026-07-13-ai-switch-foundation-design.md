# AI Switch Foundation Design

Date: 2026-07-13
Status: Approved for implementation planning

## Context

`ai-switch` will be a Tauri-based desktop application for AI provider and official account switching. The long-term product goal is to fully cover the public feature surface of `cc-switch`, add the import capabilities seen in `cockpit-tools`, add official account quota lookup, and introduce first-class batch management for imported accounts and providers.

The repository currently contains only a README and an initial commit, so Phase A starts from a clean foundation.

The project is intended to be open source and commercially usable. Implementation must follow a clean-room boundary: public behavior, public documentation, and public file formats may be studied, but non-commercial source code from `cockpit-tools` must not be copied or translated.

## Product Scope

The complete product is too large for one implementation cycle. Work will be split into phases.

Phase A builds the foundation only. It creates the app shell, storage model, backend services, frontend containers, import framework, batch-first listing model, atomic config writer, and tests required for later platform-specific work.

Later phases will implement complete provider switching, official account import and quota lookup, MCP, prompts, skills, deep links, proxying, cloud sync, usage tracking, sessions, system tray switching, multi-instance management, wakeup tasks, and plugin integration.

## Phase A Goals

- Scaffold a Tauri 2 desktop application with a React and TypeScript frontend and a Rust backend.
- Use SQLite as the local business data source of truth.
- Store device-level settings in JSON outside the database.
- Define unified models for target apps, providers, official accounts, batches, imports, config snapshots, quota snapshots, and secret references.
- Support batch-first listing: imported accounts and providers are grouped under their batch names by default.
- Support creating, editing, deleting, searching, and expanding batches, providers, and metadata-only official account records.
- Support an example JSON import flow where the user can set a batch name during import.
- Record import attempts, successes, conflicts, and failures.
- Provide an atomic config writer abstraction with snapshots and rollback-ready metadata.
- Define extension interfaces for target app adapters, importers, quota providers, and secret storage.
- Add a test skeleton that verifies the foundation behavior.

## Non-Goals For Phase A

- No real Codex, Claude, Gemini, or other official OAuth flow.
- No real official quota API integration.
- No complete target app switching implementation.
- No MCP, prompts, skills, proxy, cloud sync, usage dashboard, session manager, updater, or system tray hot-switching.
- No multi-instance app launching or wakeup automation.
- No migration from `cc-switch` or `cockpit-tools` databases.
- No guarantee that imported example data can immediately switch a real external tool.

## Architecture

The application uses a layered architecture.

Frontend:

- `src/components` contains UI components and domain screens.
- `src/lib/api` wraps all Tauri command calls with typed functions.
- `src/types` contains shared TypeScript-facing data shapes.
- `src/lib/query` configures client-side cache and invalidation.

Backend:

- `commands` is the Tauri IPC layer. It validates request shapes, calls services, and maps errors into stable response objects.
- `services` contains business rules for batches, providers, accounts, imports, settings, snapshots, and target state.
- `database` contains migrations, connection setup, and DAO/repository code.
- `adapters` contains target app integration interfaces and later platform implementations.
- `importers` contains parsing and normalization pipelines for imported data.
- `security` contains secret reference resolution and secure storage integration.
- `config_writer` contains atomic write, verification, and snapshot primitives.

Frontend code must not directly encode persistence or filesystem business rules. Backend services own mutations and validation.

## Core Domain Concepts

### TargetApp

`TargetApp` represents an external tool that can receive provider or account configuration, such as Claude Code, Claude Desktop, Codex, Gemini CLI, OpenCode, OpenClaw, or Hermes.

Phase A stores target app definitions and enabled/disabled state. Later phases attach real adapters.

### Provider

`Provider` represents a third-party or API provider configuration. It may include base URL, model mappings, provider kind, target-specific options, display metadata, and secret references.

Secrets such as API keys are not stored directly in regular provider rows. Provider rows store secret references and non-sensitive metadata.

### OfficialAccount

`OfficialAccount` represents an official login account for a platform such as Codex, Claude, or Gemini. It stores account metadata, platform, display name, email, plan metadata, quota cache references, and secret references.

Phase A supports metadata-only account records and importable mock records. Real OAuth, token refresh, and quota lookup are later-phase work.

### Batch

`Batch` is a first-class grouping object. Import flows require or suggest a batch name. A batch can contain multiple providers and official accounts.

Lists default to showing batch rows before individual accounts or providers. Expanding a batch reveals its child items. Unbatched records appear under a synthetic `Ungrouped` group in English UI text or `未分组` in Chinese UI text.

### Adapter

`Adapter` is the extension point for external tool config integration. Each adapter will eventually handle:

- Detecting the target app config location.
- Reading the current config.
- Converting a provider or account into target-specific config.
- Writing via the atomic config writer.
- Verifying the result.
- Reporting whether a restart is required.

Phase A defines the trait/interface and a mock adapter only.

### ImportPipeline

`ImportPipeline` normalizes imports from files, local config, deep links, and future OAuth flows. Phase A implements the framework and an example JSON importer. The pipeline records `import_jobs`, creates or updates records, attaches them to a batch, and reports conflicts.

### QuotaProvider

`QuotaProvider` is the extension point for official account and provider quota lookup. Phase A defines the data model and interface only. Real network calls are later-phase work.

## Storage Design

The default data directory is `~/.ai-switch/`.

Expected contents:

- `ai-switch.db` stores business data.
- `settings.json` stores device-level settings.
- `backups/` stores config backups and rotated snapshots.
- `imports/` stores optional imported source copies when enabled.
- `logs/` stores application and operation logs.

SQLite is the single source of truth for business data. JSON settings are for device-level preferences only.

## Database Tables

### target_apps

Stores target tool definitions and user visibility preferences.

Key fields:

- `id`
- `key`
- `display_name`
- `enabled`
- `sort_order`
- `created_at`
- `updated_at`

### providers

Stores provider configuration metadata.

Key fields:

- `id`
- `name`
- `kind`
- `base_url`
- `model_config_json`
- `target_options_json`
- `secret_ref`
- `status`
- `sort_order`
- `created_at`
- `updated_at`

### official_accounts

Stores official account metadata.

Key fields:

- `id`
- `platform`
- `display_name`
- `email`
- `plan`
- `account_metadata_json`
- `secret_ref`
- `quota_snapshot_id`
- `status`
- `sort_order`
- `created_at`
- `updated_at`

### batches

Stores import and manual grouping batches.

Key fields:

- `id`
- `name`
- `source`
- `notes`
- `sort_order`
- `created_at`
- `updated_at`

### batch_items

Stores many-to-many batch membership for providers and official accounts.

Key fields:

- `id`
- `batch_id`
- `item_type`
- `item_id`
- `sort_order`
- `created_at`

The valid `item_type` values in Phase A are `provider` and `official_account`.

### import_jobs

Stores import history and results.

Key fields:

- `id`
- `source_type`
- `source_label`
- `batch_id`
- `strategy`
- `status`
- `success_count`
- `failure_count`
- `conflict_count`
- `summary_json`
- `created_at`
- `completed_at`

### target_app_states

Stores current activation state by target app.

Key fields:

- `id`
- `target_app_id`
- `active_item_type`
- `active_item_id`
- `last_write_status`
- `last_error_code`
- `last_written_at`
- `updated_at`

### config_snapshots

Stores snapshot metadata for external config writes.

Key fields:

- `id`
- `target_app_id`
- `operation`
- `path`
- `before_hash`
- `after_hash`
- `backup_path`
- `status`
- `error_code`
- `created_at`

### quota_snapshots

Stores cached quota responses.

Key fields:

- `id`
- `owner_type`
- `owner_id`
- `status`
- `remaining_label`
- `reset_at`
- `summary_json`
- `raw_excerpt_json`
- `fetched_at`

### secure_secrets

Stores secret references and non-sensitive metadata only.

Key fields:

- `id`
- `provider`
- `external_ref`
- `label`
- `created_at`
- `updated_at`

The preferred secret provider is the operating system keychain. If keychain access fails, the app may use a local encrypted fallback after surfacing the reduced security model to the user.

## Batch-First Listing Rules

Default list behavior:

- Show batches first.
- Show batch name, source, child count, health summary, and latest import time.
- Expand a batch to show providers and official accounts.
- Show unbatched items under `未分组`.
- Preserve per-batch child ordering.
- Allow search across batch names, provider names, account names, emails, platforms, and source labels.
- If search matches a child item, keep its parent batch visible and expanded in the result view.

Conflict and health status:

- A batch is `ok` when all children are valid.
- A batch is `warning` when at least one child has missing optional metadata or stale quota data.
- A batch is `error` when at least one child has invalid required data, failed import state, or failed write state.

## Atomic Config Writing

All external config writes must go through `ConfigWriter`.

Write flow:

1. Resolve and validate the target path.
2. Read the current file if it exists.
3. Create a config snapshot and backup metadata.
4. Render the next content.
5. Write to a temporary file in the same directory.
6. Flush file contents.
7. Rename the temporary file over the target.
8. Verify the written content or hash.
9. Record success or failure.

If any step fails before rename, the original file must remain untouched. If verification fails after rename, the app must keep enough snapshot metadata to support a later rollback action.

## Import Flow

Phase A supports an example JSON import format for both providers and official accounts.

Import steps:

1. User selects import source.
2. User enters or confirms a batch name.
3. Importer parses the source.
4. Pipeline validates records.
5. Pipeline detects conflicts by stable identity fields.
6. User-selected strategy is applied: skip, overwrite, duplicate, or rename.
7. Records are stored.
8. Records are attached to the batch.
9. Import job result is written.
10. UI shows success, conflict, and failure details.

The import pipeline must be format-agnostic so later phases can add `cc-switch`, `cockpit-tools`, local config, and deep-link importers without changing UI flow.

## Error Handling

Every backend error exposed to the frontend must include:

- `code`
- `message`
- optional `details`
- optional `recoverable`
- optional `operation_id`

Error categories:

- `validation`: user input or malformed import data.
- `filesystem`: permissions, missing paths, atomic write failures.
- `database`: migration, constraint, or transaction failures.
- `external`: future network and external service failures.
- `secret`: keychain or encrypted fallback failures.
- `adapter`: target app detection, conversion, or verification failures.

User-facing messages should be actionable and concise. Technical detail should be available in logs or expandable UI.

## Frontend Screens In Phase A

### Dashboard Shell

Shows high-level counts for batches, providers, official accounts, import jobs, and recent errors.

### Batches

Primary list screen. Shows batch-first expandable groups and ungrouped records.

### Providers

Provider-focused list and detail drawer. Uses the same batch data but filters to providers.

### Accounts

Official account-focused list and detail drawer. Uses the same batch data but filters to accounts.

### Imports

Import entry point and import history.

### Targets

Shows target app definitions and metadata-only activation state.

### Settings

Stores app language, theme, data directory display, logging preference, and secret storage status.

### Operation Log

Shows recent import, write, validation, and adapter events.

## Testing Strategy

Rust tests:

- Database migrations run on an empty database.
- Batch repository creates, updates, deletes, and lists grouped items.
- Provider and official account repositories store records with secret references.
- Import pipeline parses example JSON and records job results.
- Conflict strategies behave deterministically.
- Atomic config writer preserves original files on pre-rename failure.
- Error mapping returns stable codes.

Frontend tests:

- Batch list renders collapsed and expanded states.
- Search matches batch names and child item names.
- Import form requires a batch name or accepts a generated one.
- Import result displays success, conflict, and failure counts.
- Settings screen loads and saves mocked settings.

Smoke test:

- App starts.
- User creates a batch.
- User imports example JSON into a named batch.
- User expands the batch and sees child items.
- User edits a setting.

## Acceptance Criteria

Phase A is complete when:

- The Tauri app starts in development mode.
- SQLite migrations are applied automatically.
- The app can create, edit, delete, and list batches, providers, and metadata-only official account records.
- Lists default to batch-first display with expandable children.
- Example JSON import supports a user-defined batch name.
- Import attempts are recorded in `import_jobs`.
- Conflicts and failures have readable UI messages and stable backend error codes.
- Atomic config write primitives exist and are covered by tests.
- The extension interfaces for adapters, importers, quota providers, and secret storage are defined.
- Rust unit tests and frontend component tests can run.

## Later Phase Breakdown

Phase B: Provider switching

- Cover `cc-switch` target apps: Claude Code, Claude Desktop, Codex, Gemini CLI, OpenCode, OpenClaw, and Hermes.
- Add provider presets, target-specific config rendering, import/export, and basic tray switching.

Phase C: Official account system

- Add official account import for Codex, Claude, Gemini, and other priority platforms.
- Add token refresh where appropriate.
- Add real quota lookup and cached quota display.
- Preserve batch grouping in account-heavy workflows.

Phase D: Advanced `cc-switch` parity

- Add MCP management.
- Add prompts and skills management.
- Add deep-link imports.
- Add local proxy, failover, usage tracking, cloud sync, session manager, and updater.

Phase E: `cockpit-tools` style automation

- Add multi-instance management.
- Add wakeup tasks.
- Add bulk operations, tags, and plugin linkage.
- Add more official IDE account importers.

## Phase A Implementation Defaults

- License: MIT, unless the user explicitly requests Apache-2.0 before implementation starts.
- Frontend styling: Tailwind CSS with a small local component layer, keeping room for shadcn-compatible primitives without requiring wholesale adoption.
- SQLite access: `sqlx` with checked migrations and repository wrappers.
- Secret storage: use the Rust `keyring` crate for OS keychain integration; use an encrypted local fallback only after surfacing the reduced security model to the user.
- Build pipeline: Phase A includes development, test, and local build scripts. Release packaging automation is deferred to a later phase.
