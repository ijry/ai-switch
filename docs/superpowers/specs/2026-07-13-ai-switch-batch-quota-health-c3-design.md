# AI Switch Batch Quota Health C3 Design

## Context

C2 added quota snapshot cache records for official accounts. Batch health still only reflects child provider/account status values and does not account for missing or unhealthy quota cache data.

C3 connects official account quota snapshots to batch health.

## Goals

- Surface quota cache health in batch groups.
- Mark account children with missing quota snapshots as `warning`.
- Mark account children with quota snapshot status `warning` or `unknown` as `warning`.
- Mark account children with quota snapshot status `error` as `error`.
- Keep provider health behavior unchanged.

## Non-Goals

- No quota network calls.
- No stale timestamp threshold yet.
- No schema migration.
- No frontend redesign.

## Health Rules

For provider children:

- use `providers.status` as before

For official account children:

- if `official_accounts.status = "error"`, child status is `error`
- else if linked quota snapshot status is `error`, child status is `error`
- else if there is no linked quota snapshot, child status is `warning`
- else if linked quota snapshot status is `warning` or `unknown`, child status is `warning`
- else child status is the account status

Batch health continues to aggregate child statuses:

- any `error` child => `error`
- else any `warning` child => `warning`
- else `ok`

## Completion Criteria

- Batch groups include quota-aware account child status.
- Batch repository tests cover missing quota and error quota cases.
- Existing Batches UI displays warning/error health without structural changes.
