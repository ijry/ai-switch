# AI Switch Sync Foundation D5 Design

## Context

D4 added local proxy, failover, and manual usage records. The next Phase D item is cloud sync. Full cloud synchronization requires remote transports, conflict resolution, encryption, and credential handling. D5 builds a safe local foundation first.

D5 does not upload, download, or contact cloud services. It stores sync profile metadata and creates local snapshot manifest records that summarize current data counts.

## Goals

- Add sync profile records for `local_folder`, `webdav`, `s3`, and `git` providers.
- Require secret references for sync credentials.
- Validate sync scope JSON before persisting profiles.
- Add sync snapshot records with item counts and a manifest JSON.
- Add a `Sync` screen for creating profiles and recording snapshot manifests.

## Non-Goals

- No network calls.
- No file upload or download.
- No conflict resolution.
- No encryption-at-rest changes.
- No background sync scheduler.

## Data Model

`sync_profiles` stores provider metadata, endpoint information, optional `auth_ref`, scope JSON, enabled state, and notes.

`sync_snapshots` stores a profile reference, direction, status, item count JSON, manifest JSON, optional artifact reference, and creation time.

## Safety Rules

- `auth_ref` must start with `env://` or `secret://` when provided.
- `scope_json` must be a JSON object.
- WebDAV endpoints must start with `http://` or `https://`.
- S3 endpoints must start with `s3://`.
- Git endpoints must start with `https://`, `ssh://`, or `git@`.
- D5 snapshot creation only records local counts and never serializes raw secrets.

## Completion Criteria

- Backend can create/list sync profiles and create/list sync snapshots.
- Snapshot manifests include provider, account, MCP, prompt/skill, routing, and usage counts.
- Tauri commands expose all D5 operations.
- `Sync` screen can create profiles and record snapshot manifests.
- Tests cover success and validation paths.
