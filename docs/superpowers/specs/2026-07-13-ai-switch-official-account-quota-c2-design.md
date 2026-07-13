# AI Switch Official Account Quota C2 Design

## Context

C1 added metadata-only official account list/create support. The database already has a `quota_snapshots` table and official accounts have `quota_snapshot_id`, but the app has no Rust model, repository, command, or UI for quota cache records.

C2 adds quota snapshot cache plumbing and a manual recording path. This creates the data path that future real quota providers can use without implementing network calls yet.

## Goals

- Add Rust models and repository helpers for `quota_snapshots`.
- Allow recording a quota snapshot for an official account.
- Attach the latest recorded quota snapshot to the official account.
- List official accounts with their cached quota snapshot.
- Show cached quota on the Accounts screen.
- Keep the workflow metadata-only and reference-only.

## Non-Goals

- No real quota API calls.
- No OAuth.
- No token refresh.
- No provider quota lookup.
- No background refresh scheduler.
- No raw token or secret storage.

## Backend

New models:

- `QuotaSnapshot`
- `NewQuotaSnapshot`
- `OfficialAccountStatus`
- `RecordAccountQuotaSnapshotRequest`
- `RecordAccountQuotaSnapshotOutcome`

New service behavior:

1. Validate the account exists.
2. Validate `status` is one of `ok`, `warning`, `error`, or `unknown`.
3. Validate `summary_json` and `raw_excerpt_json` are JSON.
4. Insert a `quota_snapshots` row with `owner_type = "official_account"`.
5. Update the account's `quota_snapshot_id`.
6. Return the account and snapshot.

Stable validation codes:

- `validation.quota_status`
- `validation.quota_summary_json`
- `validation.quota_raw_excerpt_json`

## Frontend

Accounts switches from plain account listing to account statuses:

- account identity and metadata
- cached quota status, remaining label, reset time, and fetched time
- a small manual quota form per selected account

The form records status, remaining label, reset time, summary JSON, and raw excerpt JSON.

## Completion Criteria

- Quota snapshots can be inserted and linked to official accounts.
- Accounts screen displays cached quota data.
- Accounts screen can record a manual quota snapshot.
- Rust and frontend tests cover the new flow.
