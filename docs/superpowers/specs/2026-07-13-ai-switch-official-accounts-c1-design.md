# AI Switch Official Accounts C1 Design

## Context

Phase A created the `official_accounts` table, account repository, batch linking, example JSON import, and metadata-only account model. The Accounts screen is still a placeholder.

Phase C will eventually add real account imports, token refresh, and quota lookup. C1 starts with the smallest useful slice: manual metadata-only account creation and listing.

## Goals

- List existing official account records.
- Create metadata-only official accounts from the Accounts screen.
- Preserve optional batch grouping by allowing a batch id in the backend command.
- Keep secret handling reference-only through `secret_ref`.
- Keep import/export compatibility with existing `example_json`.

## Non-Goals

- No OAuth or browser login.
- No token refresh.
- No real quota network calls.
- No raw token or key storage.
- No edit/delete workflow.
- No account switching into real target configs.

## Backend

Add `list_official_accounts` as a Tauri command over the existing `AccountRepository::list` helper.

Existing `create_official_account` remains the create command. Validation stays in `BatchService::create_official_account`:

- display name is required
- optional batch id links the account into `batch_items`

## Frontend

The Accounts screen becomes a metadata-only account manager:

- account list with platform, email, plan, status, and secret reference
- create form for platform, display name, email, plan, metadata JSON, and secret reference
- JSON metadata defaults to `{}`
- successful create invalidates account and batch queries

## Completion Criteria

- `list_official_accounts` command and API client function exist.
- Accounts screen lists records and can create a metadata-only account.
- Backend and frontend tests cover the new behavior.
- Verification passes with typecheck, Vitest, Rust check, and Rust tests.
