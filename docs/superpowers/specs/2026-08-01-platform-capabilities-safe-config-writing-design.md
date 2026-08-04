# Platform Capabilities and Safe Config Writing Design

## Status

Approved for specification on 2026-08-01. This document covers phase A only: capability truthfulness and safe configuration writes. Native OpenCode, OpenClaw, and Hermes configuration adapters remain deferred.

## Problem Statement

AI Switch currently presents seven agent platforms, but its native configuration writer supports only four:

- Codex: `~/.codex/config.toml`
- Claude: `~/.claude/settings.json`
- Gemini: `~/.gemini/settings.json`
- Grok: `~/.grok/settings.json`

OpenCode, OpenClaw, and Hermes can participate in some generic account, routing, statistics, terminal, and session flows, but they do not have native configuration writers. The UI does not consistently communicate that boundary.

Several platform normalization and request-building paths also convert unknown platforms into Codex or OpenAI behavior. This makes an unsupported platform appear to work while routing with the wrong semantics. Examples include platform normalization in `src-tauri/src/services/route_pool_service.rs`, proxy platform normalization and official base URL selection in `src-tauri/src/services/route_proxy_service.rs`, and model-test interface selection in `src-tauri/src/services/route_model_test_service.rs`.

The existing config writer uses a same-directory temporary file and atomic replacement, but it does not persist recoverable snapshots, detect external edits before committing, expose rollback, or define safe behavior for symlinks and filesystem metadata. Multi-file rollback deletes a newly created target when a later write fails, but it does not first verify that the file still contains the bytes written by the current operation.

There is no AI Switch code that reads, writes, or deletes Hermes `config.yaml`. Hermes reports must therefore be distinguishable from Hermes' own setup, migration, or update logic. Phase A must make that safety boundary explicit and regression-tested.

## Goals

1. Make backend platform capabilities the single source of truth for desktop, Web, and frontend behavior.
2. Keep OpenCode, OpenClaw, and Hermes visible as partially supported platforms while disabling unavailable operations with a concrete reason.
3. Remove implicit Codex and OpenAI fallback from platform identity, official-account behavior, quota behavior, and model-test behavior.
4. Separate agent platform identity from upstream API dialect so generic API routing remains possible without pretending that an agent is Codex.
5. Route every supported configuration mutation through one coordinator that creates secure backups, records snapshots, detects concurrent edits, performs atomic writes, and supports guarded rollback.
6. Replace static or misleading readiness claims on Accounts, Targets, Dashboard, and Operation Log with data-backed status.
7. Make the phase A command contract consistent across Tauri and Web transports.
8. Prove that every AI Switch configuration write entry point leaves Hermes configuration untouched.

## Non-Goals

- Do not implement native OpenCode, OpenClaw, or Hermes configuration adapters.
- Do not infer those platforms' configuration paths or schemas.
- Do not hard-code Hermes to `~/.hermes`; a future Hermes adapter must honor profile-aware `HERMES_HOME` resolution.
- Do not implement unsupported official-account import, official quota, or Deeplink behavior for OpenCode, OpenClaw, or Hermes.
- Do not redesign the provider/account/import data model in this phase.
- Do not fully implement the Providers page or a general-purpose operation event system.
- Do not add force-overwrite or force-rollback controls.
- Do not display snapshot contents or secrets in the UI, logs, or API responses.
- Do not automatically prune configuration backups in phase A.

## Product Behavior

### Platform Visibility

All seven platform tabs remain visible. Each platform shows a support badge derived from backend capabilities:

- Codex, Claude, Gemini, and Grok show native configuration support.
- OpenCode, OpenClaw, and Hermes show partial support.

Unavailable actions remain visible but disabled. The UI explains the missing capability, for example: "Native config writing is not implemented for Hermes." A disabled action must never be presented as a transient runtime failure that the user can fix by retrying.

### Safe Direct Write

Supported platforms retain a one-click write action. The click starts a safe write immediately; there is no second diff-confirmation dialog. The operation automatically performs backup, snapshot recording, concurrent-edit detection, atomic replacement, and result reporting.

After a successful write, the UI exposes the resulting snapshot and a guarded rollback action. Failed writes and conflicts are shown prominently rather than remaining only in mutation state.

### Fail-Closed Behavior

Unknown platform input never becomes Codex. Known but unsupported operations return a structured capability error before resolving a filesystem path, generating a proxy key, contacting an upstream service, or modifying state.

Legacy records containing an unknown platform string remain listable for diagnosis, but all mutating and network actions for those records are disabled.

## Architecture

### Platform Identity

Introduce one backend platform domain type with these canonical serialized identifiers:

- `codex`
- `claude`
- `gemini`
- `grok`
- `opencode`
- `openclaw`
- `hermes`

Parsing uses an explicit alias table for identifiers already used by AI Switch, its seeded target records, and persisted data. Parsing must use normalized exact matches, not substring matching. Any value outside the alias table returns `platform.unknown`.

Read paths that encounter an unknown legacy string return an `unrecognized` presentation value instead of failing an entire list. That presentation value cannot be passed into mutation or routing services as a valid platform.

### API Dialect

Agent identity and request format are separate types. The initial API dialect set is:

- `openai`
- `openai-responses`
- `anthropic`
- `gemini`

Platform identity chooses the route pool and capability rules. API dialect chooses request path, body shape, headers, and response parsing. An API credential for OpenCode, OpenClaw, or Hermes may route only when its saved configuration explicitly provides a supported dialect and upstream base URL. Missing values return validation errors; they do not default to OpenAI.

Proxy authentication keys remain the preferred source of platform identity. An explicit platform header may identify a known platform. URL-path inspection may help identify request dialect, but it must not silently choose the Codex route pool when platform identity is absent or unknown.

### Capability Registry

Add a backend `PlatformCapabilityRegistry` containing immutable descriptors. The frontend consumes serialized descriptors through `list_platform_capabilities`; it does not maintain a parallel platform switch statement.

Each descriptor contains:

- canonical platform ID and display name;
- overall support level: `supported` or `partial`;
- each operation's availability: `supported`, `partial`, or `unavailable`;
- a stable reason code for partial or unavailable behavior;
- constraints such as supported credential kinds or required API dialects.

The initial operation set covers:

- route credential CRUD;
- generic API routing;
- native config writing;
- official-account import;
- official-account routing;
- Deeplink import;
- official quota refresh;
- model listing and model testing;
- terminal launch;
- session discovery and resume.

Backend services call the same registry before executing an operation. Frontend gating improves clarity but is never the security boundary.

### Initial Capability Matrix

The registry starts with the behavior already verified in the repository:

| Capability | Codex | Claude | Gemini | Grok | OpenCode | OpenClaw | Hermes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Route credential CRUD | supported | supported | supported | supported | supported | supported | supported |
| Generic API routing | supported | supported | supported | supported | partial | partial | partial |
| Native config writing | supported | supported | supported | supported | unavailable | unavailable | unavailable |
| Official-account import | supported | supported | supported | supported | unavailable | unavailable | unavailable |
| Official-account routing | supported | supported | supported | supported | unavailable | unavailable | unavailable |
| Deeplink import | supported | supported | supported | supported | unavailable | unavailable | unavailable |
| Official quota refresh | supported | supported | unavailable | supported | unavailable | unavailable | unavailable |
| Model list/test | supported | supported | supported | supported | partial | partial | partial |
| Terminal launch | supported | supported | supported | supported | supported | supported | supported |
| Session discovery/resume | supported | supported | supported | supported | supported | supported | supported |

`partial` generic routing and model testing means API credentials only, with an explicit base URL and API dialect. It does not include official-account semantics.

### Target Apps and Adapters

Target app identity remains distinct from platform identity. For example, `claude_code` and `claude_desktop` both reference the Claude platform but may have different configuration paths and adapters.

Replace the unused mock-only adapter path in `src-tauri/src/adapters/mod.rs` with a production registry. A target adapter is responsible only for:

- identifying its target app and platform;
- resolving its supported configuration path;
- inspecting and parsing existing bytes;
- producing a validated mutation plan that preserves unmanaged settings.

Adapters do not create directories, write files, delete files, create backups, or update snapshot records. Codex, Claude, Gemini, and Grok are registered in phase A. OpenCode, OpenClaw, and Hermes have capability descriptors but no config adapters.

### Config Write Coordinator

Add a `ConfigWriteCoordinator` as the only component allowed to commit configuration mutations. Existing route config services build adapter input and delegate the filesystem transaction to the coordinator.

A mutation plan contains:

- operation group ID;
- target app ID and platform ID;
- resolved path;
- whether the original file existed;
- original bytes and SHA-256 hash when present;
- validated replacement bytes and SHA-256 hash;
- adapter metadata that contains no secrets.

## Safe Write Protocol

### Preflight

For every target, the coordinator performs these steps before committing any file:

1. Check the platform capability and registered target adapter.
2. Resolve and validate the target path.
3. Reject symlinks and Windows reparse points.
4. Acquire an in-process mutex keyed by the normalized absolute path.
5. Read the existing file exactly once for adapter parsing and initial hashing.
6. Ask the adapter to generate replacement bytes from that exact input.
7. Parse or otherwise validate the generated configuration before any mutation.
8. Create and sync a byte-for-byte backup when the target already exists.
9. Insert a `prepared` snapshot record before changing the target.

If backup creation or database recording fails, the target is not modified.

### Commit

Immediately before replacement, the coordinator reads the target again and compares existence and SHA-256 with the preflight state. A mismatch returns `config.concurrent_modification` and leaves both versions untouched.

The coordinator writes replacement bytes to a unique temporary file in the target directory, syncs the file, applies supported filesystem metadata, and atomically replaces the target. It then reads the committed target, verifies the expected after hash, syncs the parent directory where supported, and marks the snapshot `succeeded`.

An existing Windows target uses the replacement API intended to preserve target metadata rather than the current generic `MoveFileExW` replacement path. A new Windows target may use an atomic move. Unix writes copy standard mode bits to the temporary file before rename. The implementation must not claim to preserve extended ACLs or xattrs unless tests prove it; unsupported metadata produces a clear limitation or safe refusal rather than silent loss.

Backup files live under the AI Switch private application data directory. Unix explicitly applies user-only directory/file modes; Windows keeps backups beneath the current user's profile and relies on its inherited user ACL rather than claiming custom ACL preservation. Because agent configuration files may contain credentials, API responses and logs expose only snapshot metadata and hashes.

### Multi-Target Writes

Multi-target writes use one operation group ID. All targets complete capability checks, parsing, validation, backup creation, and prepared snapshot insertion before the first commit.

Files commit sequentially because the filesystem cannot provide one atomic transaction across paths. If a later commit fails, the coordinator attempts to restore earlier committed targets in reverse order. It restores a target only when the target's current hash still equals the after hash written by the operation. Each target records `succeeded`, `failed`, `conflict`, or rollback outcome independently.

### Rollback

Rollback is another coordinator operation, not a direct file copy or delete.

1. Load the selected successful snapshot.
2. Lock and inspect the current target.
3. Require the current hash to equal the selected snapshot's after hash.
4. Record the current state as a new prepared rollback snapshot.
5. If the original file existed, restore its exact backup bytes atomically.
6. If the original file did not exist, delete the target only when its current hash still matches the selected after hash.
7. Verify the final state and record the rollback result.

There is no force rollback in phase A. A conflict tells the user that another program changed the file and leaves the file untouched.

### Backup Retention

Phase A performs no automatic backup deletion. Configuration files are small, and preserving recoverability is safer than introducing an unproven cleanup policy. Snapshot APIs may report count and storage usage so a later explicit retention design can use real data.

## Persistence

Use a new additive migration rather than editing `202607130001_foundation.sql`. Extend `config_snapshots` with the metadata required for safe writes and grouped rollback:

- canonical platform ID;
- operation group ID;
- source snapshot ID for rollback;
- original-file-existed flag;
- non-secret metadata JSON;
- updated timestamp.

Keep the existing path, before hash, after hash, backup path, operation, status, error code, target app ID, and created timestamp fields. Add indexes for target/time and operation group queries.

Create a focused snapshot repository for prepare, success, failure, conflict, list, and lookup operations. Database errors are recoverable to the caller but fail closed before filesystem mutation whenever the required audit record cannot be created.

If the application stops after a snapshot is prepared but before its final status update, a later inspection reconciles prepared rows older than five minutes after comparing the recorded hashes with current disk state. The cutoff avoids classifying an active write as interrupted. Reconciliation never rewrites a target automatically.

## Commands and Transport

Add or extend these service-backed operations:

- `list_platform_capabilities`
- `list_target_config_statuses`
- `list_config_snapshots`
- `rollback_config_snapshot`
- `write_route_proxy_configs`, returning structured operation and snapshot outcomes

Tauri commands and the Web dispatcher call the same service or core functions. The frontend API client uses the same request and response types for both transports.

Audit every command literal invoked by `src/lib/api/client.ts`. A command must be registered in Tauri and dispatched by Web, or it must carry explicit desktop-only metadata that disables its Web UI entry point. Add a contract test that detects missing or mismatched command names.

## User Interface

### Accounts

- Load capability descriptors through one shared query/hook.
- Show the support badge on every platform tab.
- Gate config write, official import, quota, Deeplink, and model-test actions by operation capability.
- Keep unavailable controls visible and disabled with a reason.
- Require explicit API dialect and base URL before partial generic API testing or routing.
- Show structured write errors, conflicts, and rollback outcomes near the action and through the existing notification pattern.

### Targets

Turn Targets into the configuration status center. Each row shows:

- target app and owning platform;
- adapter availability;
- resolved configuration path when an adapter exists;
- file state: missing, unmanaged, managed, invalid, or conflict;
- last successful write or failure;
- snapshot history and guarded rollback actions.

OpenCode, OpenClaw, and Hermes rows show "adapter unavailable" and never expose a write or rollback button.

### Dashboard

Remove static `Ready` cards. Show only values derived from capabilities and persisted state, such as supported adapter count, partial platform count, and recent successful/failed config operations. If a value cannot be queried, omit it.

### Operation Log

In phase A, show only persisted configuration write and rollback events. Label the view accordingly. Do not claim that imports or other operations appear until those services emit real events.

### Providers and Other Deferred Screens

Pages not implemented in phase A use accurate unavailable or limited copy. They must not claim that a repository, database table, or placeholder component constitutes a finished user feature.

## Error Contract

Use stable error codes with recoverable user-facing behavior:

- `platform.unknown`
- `capability.unavailable`
- `config.adapter_unavailable`
- `config.path_unsafe`
- `config.generated_invalid`
- `config.snapshot_failed`
- `config.concurrent_modification`
- `config.atomic_replace_failed`
- `config.verify_failed`
- `config.rollback_conflict`
- `config.rollback_failed`

Error details may include platform, target ID, and path. They must not include original or generated config bytes, bearer tokens, API keys, proxy keys, or backup contents.

## Testing

### Backend Unit and Service Tests

- Verify the complete seven-platform capability matrix.
- Verify every accepted legacy alias maps to one canonical platform.
- Verify unknown and substring-like platform values are rejected instead of becoming Codex.
- Verify API dialect is independent from platform and is required for partial generic API operations.
- Verify proxy-key platform resolution preserves OpenCode, OpenClaw, and Hermes identities.
- Verify unsupported official-account import/routing, quota, Deeplink, config-write, and model-test paths fail before network or filesystem effects.
- Verify existing Codex, Claude, Gemini, and Grok merge behavior preserves unmanaged configuration.
- Verify backup and snapshot failures leave targets unchanged.
- Verify an external edit between preflight and commit produces a conflict.
- Verify rollback restores exact original bytes.
- Verify a newly created file is deleted only when its hash matches the operation's after hash.
- Verify a multi-target failure rolls back only files committed by that operation and reports conflicts independently.
- Verify prepared-operation reconciliation is read-only.

### Hermes Regression Test

Create a temporary home containing a sentinel `.hermes/config.yaml`. Invoke every public config-write path, including single-platform and write-existing-config flows. Assert that the sentinel content and SHA-256 remain unchanged. Where the test platform supports reliable timestamp assertions, also assert that the modification timestamp is unchanged.

### Filesystem Tests

- Test same-directory temporary writes and final hash verification.
- Test standard permission preservation on Unix.
- Test existing-target replacement and new-target move behavior on Windows.
- Test symlink and reparse-point refusal with platform-appropriate fixtures.
- Test temp-file cleanup after failures.

### Frontend Tests

- Verify all seven tabs render with backend-derived badges.
- Verify unavailable actions are disabled with the correct reason.
- Verify API-only partial operations require explicit dialect and base URL.
- Verify config write failures and conflicts are visibly rendered.
- Verify Targets displays real adapter and snapshot state.
- Verify Dashboard contains no unconditional `Ready` text.
- Verify Operation Log renders only returned config events.
- Verify Web-only unavailable commands are disabled rather than invoked.

### Validation Commands

- `cd src-tauri && cargo test`
- `cd src-tauri && cargo check`
- `pnpm test:run`
- `pnpm typecheck`
- `pnpm build`

## Delivery Slices

### Slice 1: Capability Truth

Add strict platform identity, API dialect separation, the capability registry, service guards, frontend capability loading, badges, and disabled reasons. This slice must remove unknown-to-Codex normalization from non-request-format logic.

### Slice 2: Routing and Test Semantics

Make proxy platform resolution preserve all seven known identities. Require explicit API credential dialect and base URL for partial platforms. Remove official base URL, quota, and model-test fallbacks for unsupported platforms.

### Slice 3: Safe Configuration Mutation

Add the migration, snapshot repository, production target adapter registry, config write coordinator, filesystem protections, grouped writes, guarded rollback, and Hermes sentinel coverage. Migrate the existing four writers without changing their managed config semantics.

### Slice 4: Truthful Surfaces and Transport Parity

Upgrade Accounts, Targets, Dashboard, and Operation Log; correct deferred-feature copy; add Tauri/Web command parity and contract tests; run the complete validation suite.

Each slice must leave the application buildable and independently testable. Implementation commits should not include the pre-existing `src-tauri/Cargo.lock`, `tauri-dev.err`, or `tauri-dev.log.err` workspace changes unless a later user request explicitly includes them.

## Acceptance Criteria

1. No unknown platform becomes Codex or receives an OpenAI official endpoint by default.
2. OpenCode, OpenClaw, and Hermes remain visible and are clearly marked partially supported.
3. Unsupported operations are disabled in the UI and rejected again by the backend.
4. API-only routing for partial platforms requires an explicit dialect and upstream base URL.
5. Every supported config write creates a secure backup and persisted snapshot before modifying a file.
6. External edits prevent write and rollback rather than being overwritten.
7. Rollback deletes a file created by AI Switch only when its current hash matches the recorded write.
8. Every AI Switch config-write entry point leaves a sentinel Hermes config byte-for-byte unchanged.
9. Targets, Dashboard, and Operation Log display only data-backed status.
10. Every frontend command used in phase A has a working Tauri and Web path or an explicit transport limitation.
11. The full Rust and frontend validation command set passes.

## Deferred Audit Backlog

The audit also identified work that is intentionally outside phase A and should receive separate specifications and implementation plans:

- Native OpenCode, OpenClaw, and Hermes adapters based on current upstream schemas.
- Provider management CRUD and switching UI.
- Unified provider, official-account, and route-credential import semantics.
- Real `skip` and `overwrite` import strategies instead of storing a strategy label without enforcing it.
- A general operation event model covering imports and non-config mutations.
- Official quota integrations for platforms with verified endpoints.
- Full Web parity for deferred features.
- Real consumers for `copy_import_sources`, `logging_enabled`, and `secret_storage` settings.
- Application-wide theme application rather than persistence-only theme controls.
- Snapshot retention and secure cleanup policy based on measured storage usage.

These items must not be presented as complete merely because a screen, setting field, table, repository, or dead adapter type exists.
