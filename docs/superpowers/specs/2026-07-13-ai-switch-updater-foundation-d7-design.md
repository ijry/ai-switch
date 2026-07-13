# AI Switch Updater Foundation D7 Design

## Context

D6 added local session management. The last Phase D item is updater support. Full update management requires network checks, package signatures, installer execution, rollback policy, and OS-specific behavior. D7 adds safe local metadata first.

D7 does not check remote feeds, download packages, run installers, or modify the running app.

## Goals

- Add update channel records for `stable`, `beta`, and `nightly`.
- Add manual update check records with current/latest versions and status.
- Add an `Updates` screen for managing channels and recording check results.
- Validate channel URLs and update statuses before persisting records.

## Non-Goals

- No network update checks.
- No auto-updater integration.
- No package download.
- No installer execution.
- No signature verification yet.

## Data Model

`update_channels` stores name, channel, feed URL, enabled state, notes, status, and timestamps.

`update_checks` stores optional channel ID, current version, latest version, status, release notes URL, details JSON, and checked time.

## Safety Rules

- Feed URLs must start with `https://` unless left empty.
- Release notes URLs must start with `https://` unless left empty.
- Details JSON must be an object.
- Status values are limited to `unknown`, `up_to_date`, `available`, and `error`.

## Completion Criteria

- Backend can create/list update channels and record/list update checks.
- Tauri commands expose all D7 operations.
- `Updates` screen can create channels and record manual check results.
- Tests cover success and validation paths.
