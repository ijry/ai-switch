---
title: Accounts and the Pool
description: A field-by-field tour of AI Switch route credentials — the status machine, priority and concurrency scheduling, in-pool/out-of-pool/archived views, batch operations, and the security rules around credential export.
---

# Accounts and the Pool

In AI Switch an "account" is a **route credential**. Every credential belongs to exactly one platform (Codex, Claude Code, Gemini CLI, Grok, OpenCode, OpenClaw, Hermes). Once it joins that platform's **pool**, the local proxy can pick between credentials, rotate through them, and back off from the ones that are misbehaving.

This page covers what a credential is made of, how its status moves, the order the scheduler picks in, and what to watch out for during bulk maintenance and export.

## Two kinds of credential

The database constrains the credential kind to exactly two values:

```sql
kind TEXT NOT NULL CHECK (kind IN ('official','api'))
```

| Kind | Where it comes from | Typical contents |
| --- | --- | --- |
| `official` | Imported from an official sign-in state (pasted text or a file) | Official OAuth credentials, subscription info, quota windows |
| `api` | Hand-entered or imported third-party API credentials | Base URL + API key + upstream protocol dialect |

Importing official credentials requires a **batch name**. The service layer first creates a batch record with source `route_credential_import`, then attaches the results to it; an empty batch name returns `validation.batch_name_required`. Batches are what let you archive or restatus "those 30 accounts I imported last Tuesday" as a group later.

Platform support varies: OpenCode, OpenClaw, and Hermes accept `api` credentials only, and require an explicit base URL and dialect. See [Platform Support Matrix](/en/guide/platform-support).

## Fields on an API credential

Secrets and non-secrets are stored separately: the API key lands in `secret_payload_json`, everything else in `config_json`.

| Field | Stored in | Notes |
| --- | --- | --- |
| Display name | `display_name` | Identifies the account in lists and logs; required |
| Base URL | `config_json.base_url` | Upstream API root; required |
| API key | `secret_payload_json.api_key` | Required; never returned in plaintext by ordinary list endpoints |
| Upstream dialect | `config_json.interface_format` | `openai` / `openai-responses` / `anthropic` / `gemini` |
| Model mappings | `config_json.model_mappings` | Array; each entry has `from`, `to`, plus optional `label` and `supports_1m` |
| Fetched model list | `config_json.fetched_models` | Written by the model-list fetch, see [Model Connectivity Tests](/en/guide/model-test) |
| Custom-tool compat | `config_json.responses_custom_tool_compat` | Boolean, defaults to `false` |
| Custom User-Agent | `config_json.headers["User-Agent"]` | Optional; sent as a fixed request header when set |
| Per-turn reminder | `config_json.turn_reminder` | Boolean; when on, a line is appended after the newest user message on every turn. The key is omitted when off |
| Reminder text | `config_json.turn_reminder_text` | Optional; falls back to the built-in default when blank |
| Failure policy | `config_json.failure_policy` | Per-account retry and semantic-failure overrides, see [Reliability and Auto Recovery](/en/guide/reliability) |
| Recovery rule | `config_json.recovery` | Scheduled or probe-based recovery, see [Reliability and Auto Recovery](/en/guide/reliability) |

**The Anthropic dialect has an auth-field choice.** `api_key_field` accepts exactly two values: `ANTHROPIC_API_KEY` (the default, sends an `x-api-key` header) or `ANTHROPIC_AUTH_TOKEN` (sends `Authorization: Bearer`). Anything else is rejected. This exists because plenty of third-party Claude-compatible gateways accept only one of the two.

What each dialect means, and how it decides the shape a request gets rewritten into, is covered in [Protocol Routing and Bridging](/en/guide/protocol-routing).

## The status machine

Status is restricted to five values (enforced by `validate_route_credential_status`):

| Status | Meaning | Scheduled? | Auto-recoverable? |
| --- | --- | --- | --- |
| `ok` | Healthy | Yes | — |
| `warning` | Under observation; something went wrong | No | Yes |
| `error` | Judged unusable | No | Yes |
| `paused` | Manually paused | No | Yes (needs an explicit trigger) |
| `revoked` | Credential is dead / has been revoked | No | **No** |

The database default is `ok`. Three classes of event drive transitions:

- **Request succeeded** — clears the transient failure count and backoff window; if the status was `error` or `warning`, it is pulled back to `ok`.
- **Request failed** — routed by failure class. A permanent failure (a revoked refresh token, say) writes `revoked` immediately; a retryable failure increments the transient counter and sets a backoff window; a semantic failure increments a fingerprint-matched streak counter and only flips to `error` once the threshold is reached.
- **Explicit test succeeded** — running a model connectivity test against a single account and passing triggers "explicit-test recovery", clearing every failure counter and backoff window.

`revoked` is the only terminal state: the reactivation SQL carries `WHERE id = ? AND status != 'revoked'`, and the auto-recovery scheduler skips it too. Reviving a revoked account means re-importing it or changing the credential itself.

**A `paused` account can still be tested explicitly.** That is deliberate: running one test is exactly how you find out whether a paused account has come back.

**Beyond account status there is a second layer of model status.** Every model on an account has its own `ok` / `error` / `paused` state and its own cooldown window, stored in `route_credential_models`. An account-level `paused` takes the whole account out of scheduling; a model-level `paused` takes out only that one model. The two are independent. The edit drawer's 模型状态 ("model status") section pauses, resumes, or clears the cooldown of each model individually.

Full failure classification, backoff durations, and thresholds live in [Reliability and Auto Recovery](/en/guide/reliability).

## Priority and concurrency limit

Both scheduling parameters are constrained at the database level:

```sql
ALTER TABLE route_credentials
  ADD COLUMN route_priority INTEGER NOT NULL DEFAULT 3
    CHECK (route_priority BETWEEN 1 AND 5);
ALTER TABLE route_credentials
  ADD COLUMN max_concurrency INTEGER NOT NULL DEFAULT 1
    CHECK (max_concurrency >= 1);
```

The column default is still 1, but the account-creation write path binds 5 explicitly (`DEFAULT_ROUTE_CREDENTIAL_MAX_CONCURRENCY`), so new accounts effectively default to 5. Existing accounts are untouched.

| Parameter | Range | Default | Effect |
| --- | --- | --- | --- |
| `route_priority` | 1–5 | 3 | Lower wins. Accounts sharing a value form one priority group |
| `max_concurrency` | ≥ 1 | 5 | Cap on in-flight requests for this account; when full, it is skipped this round |

### Scheduling order

Picking an account for one proxy request goes like this:

1. **Gather candidates.** The SQL requires: in the pool and enabled (`route_pool_members.enabled = 1`), not archived (`archived_at IS NULL`), status `ok`, and quota columns (`primary_remain`, `weekly_remain`) either null or greater than zero. This step does not judge cooldown.
2. **Sort.** `ORDER BY route_priority ASC, sort_order ASC, created_at ASC` — priority first, then your manual in-pool ordering, then creation time.
3. **Round-robin within groups.** Candidates are grouped by `route_priority` and polled from a persisted cursor (the `route_pool_cursors` table stores `next_index` per platform). Persisting the cursor means a restart does not send everything at the first account again.
4. **Filter by model and resolve the model key.** One pass against platform capability rules and the model named in the request, which also computes the upstream model key each survivor will be charged under (the mapping's `to`; official accounts keep the requested name). An empty result returns `route_pool.model_unmatched`.
5. **Read model state and drop what is cooling.** `route_credential_models` is read in one batch keyed by `(account id, model key)`, then: models that are paused or marked unhealthy are **hard-excluded** and cannot be reached by any fallback; anything whose account-level or model-level cooldown has not expired goes to the cooling bucket. If that empties the eligible set, the scheduler keeps **only the single soonest-recovering cooling account** to try, rather than failing the request outright; if not even a fallback remains (everything was hard-excluded), it returns `route_pool.model_unavailable`.
6. **Take a concurrency lease.** `try_acquire(platform, id, max_concurrency)` is attempted per candidate; failing to get a lease moves on to the next account in the retry queue.

**Step 4 must precede step 5.** Whether something counts as cooling depends on which model was asked for, so it cannot be partitioned before the model is known. Full rules in [Reliability and Auto Recovery](/en/guide/reliability).

The composite index behind this query is:

```sql
CREATE INDEX IF NOT EXISTS idx_route_credentials_routing_priority
  ON route_credentials(platform, route_priority, status, next_retry_at, cooldown_until);
```

Its column order *is* the scheduling semantics: **pools are split by platform, ordered by priority within a platform, and cooling accounts are excluded.** Model-level state has an index of its own:

```sql
CREATE INDEX IF NOT EXISTS idx_route_credential_models_lookup
  ON route_credential_models(route_credential_id, status, cooldown_until);
```

### How to actually set these

- **Primary plus standby**: primaries at 1 or 2, standbys at 4 or 5. Traffic only reaches the standbys once every primary is cooling or saturated.
- **Flat weighting**: put everything at one priority and let round-robin spread quota consumption evenly.
- **Match concurrency to the upstream limit**: if the upstream caps concurrency per key, set `max_concurrency` to what it allows. New accounts start at 5; drop a concurrency-sensitive upstream to 1 to go back to one in-flight request per account at any moment.

## List views and batch operations

The account list pages through three mutually exclusive scopes (`RouteCredentialPoolScope`):

| Scope | Meaning |
| --- | --- |
| `in_pool` | Joined this platform's pool |
| `out_of_pool` | Exists but is not in the pool (the default scope) |
| `archived` | Archived |

Page size accepts only `20`, `50`, and `100`; anything else is rejected with `page_size must be 20, 50, or 100`.

Available single and batch operations:

| Operation | Behaviour |
| --- | --- |
| Archive / unarchive | Bulk-writes or clears `archived_at`; archived accounts are excluded from all scheduling |
| Set status in bulk | Writes one valid status across the selected IDs |
| Drag to reorder | Recomputes `sort_order` from a "move between these two accounts" gesture, within the current filter and scope |
| Duplicate | Copies a credential; the new name gets a `YYYY-MM-DD` date stamp appended |
| Delete | Hard delete; the pool membership table has `ON DELETE CASCADE`, so membership rows go with it |

**Archive versus delete**: archiving is a reversible soft-hide — the credential and its usage history survive. Deleting is not reversible. When you rotate a batch of accounts out, archive is the right tool.

Archiving has its own composite index:

```sql
CREATE INDEX IF NOT EXISTS idx_route_credentials_archive
  ON route_credentials(platform, archived_at, sort_order);
```

## Export and import

The export dialog offers two formats: a **JSON file** and **scheme links**.

::: danger Exporting means exposure
This export contains credentials. Store it securely and remove copies you no longer need.
:::

### JSON export

- The suggested file name looks like `ai-switch-<platform>-route-credentials-20260819-101530.json` — the platform and a UTC timestamp are both in the name.
- The payload carries `schema_version: 1` plus metadata: source instance ID, source credential ID, platform, kind. You can turn off "enhanced metadata" to export core fields only.
- One export covers at most **2000** credentials and **8 MiB** serialized (`8 * 1024 * 1024`). Over the limit returns `transfer.selection_too_large` or `transfer.export_too_large`.
- Desktop uses the system save dialog; the web service mode uses a browser download.

### Scheme links

A scheme link is a deep link of the form `aiswitch://v1/import?...`, and it is **generated for `api` credentials only** (official credentials have no equivalent that fits in a URL).

::: warning Copying scheme links
Copying scheme URLs places API keys on the system clipboard.
:::

Hitting copy raises a confirmation first, worded:

> This scheme URL contains an API key. Copy it to the system clipboard?

Only after you confirm does anything reach the clipboard. Closing the export dialog immediately wipes the sensitive state the UI was holding.

### Import de-duplication

The import side records each credential's origin identity so re-importing the same export does not pile up duplicates:

```sql
CREATE TABLE IF NOT EXISTS route_credential_transfer_origins (
  route_credential_id TEXT PRIMARY KEY,
  source_instance_id TEXT NOT NULL,
  source_credential_id TEXT NOT NULL,
  source_platform TEXT NOT NULL,
  source_kind TEXT NOT NULL,
  source_schema_version INTEGER NOT NULL,
  source_fingerprint TEXT NOT NULL,
  imported_at TEXT NOT NULL,
  UNIQUE(source_instance_id, source_credential_id, source_platform)
);
```

Each installation also has its own stable identity (`transfer_installation_identity`), which is how "exported on machine A, imported on machine B" stays distinguishable from "imported twice on the same machine".

Beyond its own format, import also accepts export formats from other account switchers (a compatible import protocol). A `schema_version` mismatch returns `transfer.schema_version_unsupported` rather than guessing at field meanings.

## Where this is stored

All credential data lives in the app's SQLite database. The data directory is fixed at `~/.ai-switch` under your home directory, and the database file is `ai-switch.db` (development builds use a separate `ai-switch-dev.db`, so the two never collide).

::: danger The data directory *is* a credential directory
API keys and official sign-in credentials are stored in that SQLite database (the `route_credentials.secret_payload_json` column), with **no additional encryption at rest**. Treat all of `~/.ai-switch` as a credential directory:

- Keep it out of public repositories, unencrypted sync folders, and shared drives
- Mind the security of your backup media, and tighten the directory's file permissions
- Prefer a machine with full-disk encryption enabled
:::

The schema is defined by the **23** forward-only migrations under `src-tauri/migrations`. The ones relevant to this page:

| Migration | Contents |
| --- | --- |
| `202607130011_route_credentials.sql` | The `route_credentials` table and the `route_pool_members` membership table |
| `202607300001_route_credential_retry.sql` | Transient failure count, `next_retry_at`, `cooldown_until` |
| `202608040001_route_credential_transfer.sql` | Installation identity and import-origin table |
| `202608050001_route_credential_archive.sql` | `archived_at` and the archive index |
| `202608060002_route_usage_breakdown.sql` | Token and price breakdown columns on `usage_events` |
| `202608080002_route_credential_priority_concurrency.sql` | `route_priority`, `max_concurrency`, and the scheduling index |
| `202608130001_route_credential_semantic_failure_streak.sql` | Semantic-failure streak count and fingerprint |

## Next

- [Protocol Routing and Bridging](/en/guide/protocol-routing) — how an account's dialect determines the shape a request is rewritten into
- [Model Connectivity Tests](/en/guide/model-test) — how to prove a new credential actually produces output
- [Usage and Request Stats](/en/guide/usage-stats) — where per-request tokens and cost are recorded
- [Reliability and Auto Recovery](/en/guide/reliability) — the exact backoff, cooldown, and recovery rules
