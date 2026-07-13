# AI Switch Official Account Quota Refresh C5 Design

## Context

C2 added cached/manual quota snapshots for official accounts, but it still did
not query any quota source. The original product requirement includes official
account remaining quota lookup.

## Goals

- Add an explicit refresh command for official account quota snapshots.
- Support a safe generic HTTPS JSON quota endpoint configured in account
  metadata.
- Use environment-variable references for endpoint auth instead of storing raw
  tokens.
- Parse endpoint responses into the existing `quota_snapshots` table.
- Redact sensitive keys from stored raw quota excerpts.
- Add Accounts UI and tests for refresh.

## Metadata Contract

Accounts can opt in with this metadata shape:

```json
{
  "quota_query": {
    "endpoint_url": "https://quota.example.com/accounts/team",
    "auth_env_key": "TEAM_QUOTA_TOKEN",
    "auth_scheme": "Bearer"
  }
}
```

The endpoint response should be JSON:

```json
{
  "status": "ok",
  "remaining_label": "80% remaining",
  "reset_at": "2026-07-14T00:00:00Z",
  "summary": { "window": "daily" }
}
```

`status` must be `ok`, `warning`, `error`, or `unknown`. `remaining_label`,
`reset_at`, and `summary` are optional.

## Non-Goals

- No scraping private web/session APIs.
- No OAuth or token refresh.
- No raw token persistence.
- No provider-specific quota API claims where public stable APIs are unknown.
- No background refresh scheduler.

## Completion Criteria

- `refresh_official_account_quota_snapshot` records a linked quota snapshot.
- Missing or invalid metadata returns stable validation errors.
- Network failures return stable adapter errors.
- Accounts UI exposes manual record and endpoint refresh paths.
- Rust and frontend tests cover parsing, redaction, API wiring, and UI action.
