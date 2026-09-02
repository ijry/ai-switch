---
title: Reliability and Auto Recovery
description: The exact rules behind AI Switch failure classification and its two-layer account/model cooldowns — how accounts and individual models return to the pool, and how to configure scheduled versus healthcheck recovery.
---

# Reliability and Auto Recovery

The value of a multi-account pool is not "many accounts" — it is that **when one account breaks, traffic routes around it by itself, and when the problem passes, the account comes back by itself**. This page spells out AI Switch's failure classification, backoff durations, cooldown windows, and recovery rules. Every number here comes from the code; none of them are suggestions.

Cooldown has two layers: **per model** is the default, **per account** is the escalation. Relays routinely throttle one model while still serving another on the same key, so parking the whole account would take out models that work. By default only the `(account, model)` pair that failed is parked.

## The failure state in the database

Failure state is recorded at two levels: **account level** in a group of columns on `route_credentials`, and **model level** in `route_credential_models`, one row per `(account, model)` pair. One account often serves several models through `config_json.model_mappings` while the relay throttles just one of them, so the two levels have to be tracked separately.

### Account level: the failure columns on `route_credentials`

These columns were filled in by three migrations:

```sql
-- 202607300001_route_credential_retry.sql
ALTER TABLE route_credentials ADD COLUMN transient_failure_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE route_credentials ADD COLUMN next_retry_at TEXT;
ALTER TABLE route_credentials ADD COLUMN cooldown_until TEXT;
ALTER TABLE route_credentials ADD COLUMN last_failure_kind TEXT;
ALTER TABLE route_credentials ADD COLUMN last_failure_message TEXT;

-- 202608080001_route_credential_failure_response.sql
ALTER TABLE route_credentials ADD COLUMN last_failure_response_json TEXT;

-- 202608130001_route_credential_semantic_failure_streak.sql
ALTER TABLE route_credentials ADD COLUMN semantic_failure_streak_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE route_credentials ADD COLUMN semantic_failure_streak_fingerprint TEXT;
```

| Column | Purpose |
| --- | --- |
| `transient_failure_count` | Consecutive transient failures; drives the 错误 N 次 status tag |
| `next_retry_at` | The earliest moment a retry is allowed |
| `cooldown_until` | When the cooldown expires |
| `last_failure_kind` | Failure classification label, see below |
| `last_failure_message` | Failure message, stored truncated |
| `last_failure_response_json` | The upstream's raw failure response, up to **8192** characters, with `…` appended past that |
| `semantic_failure_streak_count` / `_fingerprint` | Streak length for one kind of semantic failure, and its fingerprint |

These columns now explicitly mean **account-level** state: only a failure judged account-scoped writes their cooldown timestamps (see the scope table below).

### Model level: `route_credential_models`

```sql
-- 202609020001_route_credential_models.sql
CREATE TABLE IF NOT EXISTS route_credential_models (
  route_credential_id TEXT NOT NULL,
  model_key TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'ok' CHECK (status IN ('ok', 'error', 'paused')),
  transient_failure_count INTEGER NOT NULL DEFAULT 0,
  cooldown_until TEXT,
  semantic_failure_streak_count INTEGER NOT NULL DEFAULT 0,
  semantic_failure_streak_fingerprint TEXT,
  last_failure_kind TEXT,
  last_failure_message TEXT,
  last_failure_response_json TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (route_credential_id, model_key),
  FOREIGN KEY (route_credential_id) REFERENCES route_credentials(id) ON DELETE CASCADE
);
```

| Column | Purpose |
| --- | --- |
| `route_credential_id` | Owning account; rows are cascade-deleted with it |
| `model_key` | The **upstream** model name, i.e. the mapping's `to`; primary key together with the account id |
| `status` | `ok` / `error` (set automatically by a semantic streak) / `paused` (set only by the user) |
| `transient_failure_count` | This model's own consecutive failure count |
| `cooldown_until` | When this model's cooldown expires |
| `semantic_failure_streak_count` / `_fingerprint` | This model's streak length and fingerprint |
| `last_failure_kind` / `last_failure_message` / `last_failure_response_json` | This model's most recent failure classification, message, and raw response |
| `created_at` / `updated_at` | Row creation and last update; `updated_at` decides which model a healthcheck probes first |

**Only one timestamp.** The account level writes `next_retry_at` and `cooldown_until` the same value, which is redundant; the model row does not copy that redundancy.

**The key is the upstream name, not the requested one.** For `api` accounts it is the `to` resolved from `model_mappings`; for `official` accounts and accounts with no mappings it is the requested name (official requests never rewrite the model). Both then have any `[1m]` suffix stripped, so `claude-sonnet-alias[1m]` and `claude-sonnet-alias` share one record — same upstream model, only a different beta header. Using `to` also converges naturally: under a catch-all mapping every client-side name funnels into one key, so a single failure parks it for all of them and stray model names cannot inflate the table.

**Row lifecycle.** A row is created on the first failure or a manual pause. On success a `paused` row keeps its status and only has its failure fields reset, while every other row is deleted outright. So in a healthy system this table holds nothing but manual pauses. The row count is bounded by "accounts × models mapped per account", so there is no GC.

## Three classes of failure

Once the proxy has the upstream result, it classifies before it acts. The classification function returns exactly three things:

```rust
pub enum ProxyFailureKind {
    Transient,
    Permanent,
    None,
}
```

### Permanent

This decision is made purely on error text; matching any of these substrings makes it permanent:

- `invalid_grant`
- `refresh token has been revoked`
- `token has been revoked`
- `官方 oauth 凭证已失效`

Such a failure means the credential itself is void, so retrying is meaningless. The account status is **written directly to `revoked`** with no backoff window — because it is not going to fix itself.

### Transient

The following count as transient:

- HTTP status **408 Request Timeout**, **401 Unauthorized**, **403 Forbidden**, or **429 Too Many Requests**
- Any **5xx** server error
- **No HTTP status at all** — connection failure, DNS failure, timeout, TLS error, and other transport-layer problems

401 and 403 are treated as transient because third-party gateways routinely use them to mean "this key is temporarily throttled or rate-limited", not necessarily "this key is dead". Genuinely void credentials are caught by the permanent rule above.

### Not a failure (None)

There is an HTTP status, but it is neither in the retryable list nor a 5xx — for instance 400 for a bad parameter, or 404 for a nonexistent path. These are problems with **the client request itself**, unrelated to account health, so no failure counter increments and the account status is untouched.

::: tip Why the distinction matters
If 400 counted as an account failure, one client with a misspelled model name could put the entire pool into cooldown within a minute. With the distinction in place, a parameter error is simply returned to the client and the pool is unharmed.
:::

## Semantic failure: HTTP 200 with a failure body

Third-party gateways have a common flaw: quota exhausted, upstream errored, content moderation intercepted — and what comes back is **HTTP 200**, with the failure buried in the response body. Looking only at the status code would count these as successes, and the account would never be backed off.

So there is a second layer of semantic-failure detection that parses the body to decide "this was actually a failure". Two triggers:

1. **The body is structurally a failure** (decided by the response-body failure detection service).
2. **A streaming request disconnected before the completion event** — the SSE stream broke mid-flight with no terminating event.

Semantic failures are marked as failures in the usage event (`metadata_json.success = false`) and affect account status per the rules below.

### Quota exhaustion takes a separate channel

If a semantic failure is further identified as **quota exhaustion**, it is handled differently from an ordinary semantic failure: it records one semantic failure with a threshold of **1** — meaning **a single occurrence flips the status to `error`**, with no streak allowance. The reasoning is direct: quota exhaustion is a deterministic fact, and retrying only wastes time.

This channel also clears `transient_failure_count`, `next_retry_at`, and `cooldown_until`: the account is not "retry later", it is "don't use this one for the rest of the cycle".

### The fingerprint streak mechanism

The semantic-failure streak counter does not simply increment; it increments **only on a fingerprint match**:

```rust
fn semantic_failure_fingerprint(response_status: Option<u16>, message: &str) -> String {
    // sha256("semantic_response_failed|{status or none}|{whitespace-normalized lowercase message}")
}
```

The message is split on whitespace, rejoined with single spaces, and lowercased, then hashed with SHA-256 together with the status code. The rules are:

- **Fingerprint matches the last one** → the streak count increments (capped at the threshold)
- **Fingerprint differs** → the streak count resets to 1 and the new fingerprint is stored
- Reaching the threshold flips the status to `error`
- Accounts already `revoked` or `paused` are **never modified by this rule**

The point of the fingerprint is to distinguish "the same illness recurring" from "a different one-off error each time". The former means the account is genuinely broken; the latter is more likely upstream jitter.

### The threshold governs models, not accounts

An account's failure policy has a `semantic_error_threshold` field (default 10, accepts 1–1000), and its **only consumer is the model-level streak**: when forwarding or a model test records a model-scoped failure, the streak on that model's row is incremented by fingerprint, and reaching the threshold on the same fingerprint flips that model's `status` to `error`. From then on it is hard-excluded from selection until it is cleared manually or by recovery.

The account-level streak still has only one user — the quota-exhaustion channel — and that one hardcodes the threshold to 1 (a single occurrence flips the account to `error`); it never reads the field. `error_status_enabled` is the shared master switch: turn it off and streaks keep counting but nothing is flipped to `error` automatically.

::: tip Why models need a threshold and accounts do not
With only a cooldown, a model the account simply does not support churns forever: cool for 10 seconds, retry, same error, cool again. A streak plus a threshold turns "the same failure over and over" into "stop trying".
:::

**One deliberate divergence from the account level.** The two account-level functions are mutually exclusive: recording a transient failure zeroes the streak, and recording a semantic failure clears the cooldown. The consequence is that a cooling object can never accumulate a streak, so the threshold is never reached. Model rows **accumulate both**: every model-scoped failure writes a cooldown *and* increments the fingerprinted streak.

## Outbound timeouts: turning a stalled upstream into a failure

Every classification above assumes the upstream eventually **produces** something — a status code or an error. But there is a third way for it to behave: **stalling**. The TCP connection is established, and then not a single byte arrives. No error, no close.

With no deadline, forwarding never receives an `Err` in that case, so same-account retry, failover and backoff all fail to happen and the client hangs indefinitely. Outbound requests therefore carry two ceilings:

| Ceiling | Value | What it bounds |
| --- | --- | --- |
| Connect timeout | 20 seconds | The TCP/TLS handshake. If it cannot connect, there is nothing to wait for |
| Read timeout | 180 seconds | The maximum gap between successful reads, **reset after each read** |

When one fires it becomes an ordinary transport-layer transient failure: logged as `transport`, then same-account retry or failover according to `failure_policy`, then the account's configured failure cooldown window. The failure message names which ceiling fired and how it was configured, so a stall is distinguishable from a refused connection.

::: tip Why there is no total deadline
A legitimately long answer can take minutes either way: buffered forwarding waits for the whole body to arrive, and streaming passthrough waits for the upstream to finish talking. A total "connect until fully read" deadline would kill valid generations.

A read-gap timeout has no such problem: as long as the upstream is still emitting bytes (SSE deltas, keepalives), the clock keeps resetting. It bounds "the bytes stopped", not "how long this took".
:::

## Streaming passthrough and truncation

By default the proxy **buffers the whole response body** before returning it. That is what lets the retry loop switch accounts while the client has still received nothing — including for failures that only surface once the entire body has been read.

The cost is no TTFT: the client waits for the upstream to finish before seeing a first token. So when nothing downstream needs the complete body, the proxy switches to **streaming passthrough** instead. Five conditions must all hold:

| Condition | Why |
| --- | --- |
| No protocol bridge | A bridge rewrites the body wholesale, and five of the seven do it by aggregating the entire stream before re-emitting it |
| The client asked for a stream | A non-streaming reply is one JSON document: no frames to inspect incrementally, and nothing to gain |
| The status is 2xx | A non-2xx body decides retry classification, which must happen before anything reaches the client |
| No custom tools | Custom tool restoration rewrites frames on the way out |
| Not an official account | Official accounts parse the body for subscription/quota signals |

In practice that means the two most common paths — **Claude straight to an Anthropic upstream, and Codex straight to a Responses upstream** — stream, while everything else keeps the buffered path.

### Failover still works before the first chunk

The streaming path does not hand the response over as soon as the headers arrive. It first **waits for the first data chunk inside the retry loop**, and only then gives the response to the client. So these three cases still switch accounts, exactly as on the buffered path:

- the connection dies or times out before the first chunk;
- the upstream returns 200 and then closes without sending any body;
- the first chunk is itself a failure envelope (gateways commonly put the error in the opening frame).

### After the first chunk, truncation is recorded but not retried

Once bytes are with the client they cannot be recalled, so a failure exposed after that point can no longer trigger failover. Account health is still charged, though: if the stream ends having sent data frames but never a terminal event (`response.completed` / `[DONE]` / `message_stop` / `finish_reason: stop` / `finishReason: STOP`), it is recorded as a `semantic_response_transient` failure and enters the account's configured failure cooldown window.

The terminal markers are byte-identical to the buffered path's `stream_disconnected_before_completion`, so the two paths never reach different verdicts. Only the disposition differs: the buffered path can retry, the streaming path can only record — and recording is what makes the next selection avoid a chronically truncating upstream.

::: tip Usage is not lost to streaming
Tokens and cost are settled when the stream ends, including when the client disconnects early — a half-delivered response still counts toward the stats rather than vanishing.
:::

## The exact backoff and cooldown rules

Cooldown has two layers: **per model** by default, **per account** as an escalation. The decision looks only at the failure classification and the status code, defined once and shared by every recording site:

```rust
pub(crate) fn is_account_scoped_failure(kind: &str, status: Option<u16>) -> bool {
    match kind {
        // The credential itself, or the path to the upstream, is at fault.
        "refresh" | "request_build" | "transport" | "model_test" => true,
        // A rejected key rejects every model, so settle it once at the account
        // level; every other status is the upstream's verdict on one model.
        "upstream_status" | "model_test_status" => matches!(status, Some(401) | Some(403)),
        // The upstream answered about this specific model.
        "semantic_response_transient" | "response_transform" => false,
        // Unknown kinds park the account: over-parking is recoverable, letting a
        // broken credential keep serving is not.
        _ => true,
    }
}
```

| Failure class | Scope | Why |
| --- | --- | --- |
| `refresh` / `request_build` / `transport` / `model_test` | Account | The credential itself or the path to the upstream is at fault, regardless of the model |
| **401 / 403** on `upstream_status` / `model_test_status` | Account | A rejected key rejects every model; charging each one separately would just make the account fail N times before it settles |
| Any other status on `upstream_status` / `model_test_status` (400/404/408/429/5xx) | Model | This is the upstream's verdict on that one model |
| `semantic_response_transient` / `response_transform` | Model | The upstream's response content for that model is the problem |
| Quota exhaustion | Account | Quota is an account property |
| Unknown class | Account | Better to over-park: over-parking is recoverable, letting a broken credential keep serving is not |

**Without a model name it degrades to account scope.** Gemini puts the model in the URL path, and some routes carry none at all; such requests do not consult model state during selection and charge failures to the account, so no protection is lost.

### What each layer writes

- **Model-scoped failure**: writes that one row in `route_credential_models` — increments its `transient_failure_count`, writes its `cooldown_until`, accumulates its fingerprinted streak. At the account level only `transient_failure_count` and `last_failure_kind/message/response_json` are updated; **no account-level cooldown timestamp is written**, so the account's other models stay usable.
- **Account-scoped failure**: exactly as before — the account's `next_retry_at` and `cooldown_until` are both set to `now + configured seconds`.
- **Escalation**: after a model-scoped failure, if **every serviceable model on this account is now unavailable**, an account-level cooldown is written too. That check runs **inside the same transaction** as the model row write; otherwise concurrent requests would each see "not all parked yet" and none would escalate.

The escalation denominator **excludes manually paused models**: pausing three of four must not let the fourth's single failure fake an account-wide outage. It follows that an account mapping only one model behaves exactly as it did before this feature.

For `official` accounts and accounts with no mappings the denominator is the platform baseline model set (4 each for codex and claude, 1 each for gemini and grok), so a wholly broken relay needs one failure per model before such an account escalates. Bounded, and acceptable.

### Configuring the cooldown window

The code that writes a transient-failure cooldown reads one config for both layers:

```rust
let policy = RouteCredentialFailurePolicy::from_config_json(&config_json);
let cooldown_seconds = policy.cooldown_enabled.then_some(policy.cooldown_seconds);
```

Cooldown is **per account** and off by default, and its length is **configured per account** too: edit it under the account's failure policy panel as 失败冷却（秒） ("failure cooldown, seconds"). The default is **10 seconds**, and the accepted range is 1-86400 seconds. There is **no per-model cooldown length or threshold**: `failure_policy` is one account-level record that applies to all of that account's models. Split into two accounts when you genuinely need different values.

| Account setting | Effect of each transient failure |
| --- | --- |
| Failure cooldown off | Both layers only increment their failure counters; no cooldown timestamp is written anywhere, so account and model stay immediately selectable |
| Failure cooldown on | A model-scoped failure writes the model row's `cooldown_until`; an account-scoped failure (or an escalation) sets the account's `next_retry_at` and `cooldown_until` to `now + configured seconds` |

Three things to note:

- **The cooldown window is fixed; it no longer escalates.** The old ladder charged roughly 30 seconds, then 2 minutes, then 10 minutes from the 3rd failure on. Now every trigger waits only the configured short window, so a single hiccup costs seconds instead of minutes. An account that keeps failing keeps re-triggering the same short cooldown rather than being pushed further and further out.
- **The account's two timestamps are written together.** They behave identically for scheduling (each must be expired for the account to be usable); writing both means the UI can show "cooling" and the remaining time from the very first failure.
- **Every account-scoped transient failure clears the account-level semantic streak** (`semantic_failure_streak_count = 0`, fingerprint nulled). Model rows are unaffected by that rule — their cooldown and streak accumulate together.

### The failure count and model badge in the UI

Whenever the account's `transient_failure_count` is above 0, its status tag renders as 错误 N 次 ("N errors"). The moment the latest request succeeds the counter is cleared and the tag returns to the normal status text. Terminal states — revoked, error, paused — keep their own labels and are never masked by the failure count.

The row's 冷却 N 秒 ("cooling for N seconds") badge now explicitly means an **account-level** cooldown. When models are unavailable, an extra orange badge 模型 N 不可用 ("N models unavailable") appears; the count is "cooling and not yet expired" plus `error` plus `paused`. The two badges do not overlap in meaning: one says "the whole account is backing off", the other "some models are unavailable". Hovering 模型 N 不可用 expands a per-model detail panel: the upstream model name, the client-facing aliases in parentheses (rows whose mapping was deleted show 已移除映射, "mapping removed"), the reason and remaining time, and the most recent failure message.

The edit drawer has a 模型状态 ("model status") section listing every known model — including ones that have never failed, so a healthy model can be paused pre-emptively. Each row carries two actions: a 暂停 / 恢复 ("pause" / "resume") toggle, and 解除 ("clear") to drop that model's cooldown and error state. The section header's 全部解除 ("clear all") only issues requests for models that actually have something to clear: it skips `paused` ones (that is the user's own decision) and skips healthy models with no cooldown and no failure counts.

### The sensitive-word reminder

When `sensitive_words_detected` shows up in `last_failure_response_json` or `last_failure_message`, the status tag's hover panel gains one extra line:

> 友情提醒：当前中转站似乎对项目存在关键词检测，您的项目可能存在敏感词，也不排除是中转站误判。 ("Heads-up: this relay appears to run keyword detection against your project. Your project may contain a flagged word, though a relay false positive is also possible.")

This class of error comes from the relay's own keyword filter, not from a broken account. When you see the reminder, check your prompts and code for trigger words, or try another relay to see whether it was a false positive.

## Failure classification labels

`last_failure_kind` records which step of the chain the failure occurred in, and it is used both in the UI and while debugging:

| Label | When it fires | Cooldown scope |
| --- | --- | --- |
| `refresh` | Refreshing an official credential's access token failed (and was judged transient) | Account |
| `request_build` | Building the upstream request failed (bridging, auth assembly, and so on) | Account |
| `transport` | The request could not be sent, the response body could not be read to completion, or the upstream stalled (connection, DNS, TLS, 20s connect timeout, 180s read timeout) | Account |
| `response_transform` | The upstream responded but the bridge's reverse conversion failed | Model |
| `upstream_status` | The upstream returned a retryable non-2xx status | Account on 401/403, model otherwise |
| `semantic_response_transient` | Semantic failure, taking the transient-backoff route | Model |
| `semantic_response_failed` | Semantic failure, taking the fingerprint-streak route (the quota-exhaustion channel) | Account |
| `model_test_status` | A model test received a non-2xx status | Account on 401/403, model otherwise |
| `model_test` | Any other retryable model-test failure (transport level) | Account |

The same label can appear in both the account-level and the model-level `last_failure_kind`: the account level records "this account's most recent failure", the model level "this model's most recent failure".

## Retry the same account, or switch

A failure does not necessarily mean switching accounts immediately. The forwarding logic maintains a retry queue, and the rule is:

1. Read the account's failure policy (`config_json.failure_policy`) for `retry_count` and `retry_interval_ms`.
2. If this account still has retries left, **wait `retry_interval_ms` and push it back to the head of the queue** — the same account tries again, and this attempt does not record a failure.
3. Only once the retries are exhausted does it record one transient failure and move on to the next candidate.

**401 / 403 are the exception: never retried on the same credential.** The rule is one line:

```rust
pub(crate) fn should_retry_same_credential_status(status: StatusCode) -> bool {
    !status.is_success() && !matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN)
}
```

Retrying an auth failure only trips upstream abuse controls sooner, so it records the failure and switches accounts straight away.

### Failure policy bounds

| Field | Default | Maximum |
| --- | --- | --- |
| `retry_count` | 2 | 10 |
| `retry_interval_ms` | 200 | 60000 |
| `semantic_error_threshold` | 10 | 1000 (minimum 1; 0 is rejected) |

Supplying only some fields leaves the rest at their defaults. Omitting `failure_policy` entirely uses all defaults. Out-of-range values return `validation.route_credential_failure_policy` when creating or updating an account.

## How cooling accounts and models get skipped

Selection is five steps, and the order cannot be changed:

1. **Load candidates** (`load_pool_candidates`): SQL-level filters and quota only — in the pool and enabled, not archived, `status = 'ok'`, quota columns null or greater than zero. This step does **not** judge cooldown.
2. **Filter by platform capability rule** (`filter_candidates_for_rule`): some platforms only accept `api` credentials, for instance. An empty result returns `No enabled route credentials in pool`.
3. **Filter by requested model and resolve the model key** (`filter_candidates_for_model`): one pass decides whether the account serves the requested model *and* computes the `model_key` it will be charged under. An empty result returns `route_pool.model_unmatched`.
4. **Batch-read model state** (`load_candidate_model_states`): one query for the whole pool, keyed by `(account id, model_key)`. Two accounts can map the same requested model to different upstream names, so the key must be the pair, never the model alone.
5. **Partition** (`partition_by_cooldown`): see below. An empty result returns `route_pool.model_unavailable`.

**Why model filtering must precede cooldown partitioning.** Whether something counts as cooling depends on which model was asked for, so it cannot be partitioned before the model is known. Reordering also fixes a pre-existing defect: suppose account A serves only `glm-5.3` and is healthy, while account B serves only `gpt-5.6-sol` and is cooling. A `gpt-5.6-sol` request under the old order saw a non-empty eligible set and returned `[A]`, then model filtering dropped A too and the request failed with `route_pool.model_unmatched` — even though B was the only account that could serve it and should have been the last-resort probe.

### The partition decision order

```rust
pub fn partition_by_cooldown(
    candidates: Vec<PoolCandidate>,
    model_states: &HashMap<(String, String), RouteCredentialModelState>,
    now: DateTime<Utc>,
) -> Vec<SelectedCredential>
```

1. That model's status is `paused` or `error` → **dropped outright**, into no bucket, and not eligible for the probe fallback below.
2. The account-level `cooldown_until` **or** the model-level `cooldown_until` has not expired → into the cooling bucket (the later of the two timestamps is its recovery time).
3. Otherwise → into the eligible bucket.

A non-empty eligible bucket is used as-is. The selection SQL itself already excludes anything other than `status = 'ok'`, so **accounts** flipped to `error` or `revoked` never enter the candidate set at all.

### The fallback when the whole pool is cooling

If filtering leaves **no usable account whatsoever**, the scheduler does not simply fail the request — it **picks the single soonest-recovering cooling account** and tries that:

```rust
cooling.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
Ok(cooling.into_iter().take(1).map(|(_, _, credential)| credential).collect())
```

The sort key is "earliest recovery time", with a stable tiebreak on the original order. This is a deliberate trade-off: better to try one possibly-still-cooling account than to hand the client an error, because the backoff duration is only an estimate and the upstream may already have recovered.

**Only time-based cooldowns get that second chance.** `paused` and `error` are verdicts rather than waits, so they stay excluded — consistent with account-level `status != 'ok'` being dropped by the selection SQL.

### Two model-related error codes, not to be confused

| Code | Meaning | What to do |
| --- | --- | --- |
| `route_pool.model_unmatched` | **No account maps this model at all** — step 3 emptied the set | Add it to an account's model mappings |
| `route_pool.model_unavailable` | Accounts do serve this model, but on all of them it is **paused or marked unhealthy** | Clear the pause/error in the edit drawer's 模型状态 section |

A cooldown alone never produces `model_unavailable`: a cooling candidate can still be chosen by the probe fallback, so only hard exclusions can empty the set. Merging the two codes would turn diagnosis into guesswork, which is why they are separate.

::: warning The client always sees 502
The proxy does not pass upstream status codes through. Both codes above, and "every account failed", come back as HTTP **502**; the actionable information is `error.code` in the body. So when the upstream returns 429, the client sees 502, not 429.
:::

**The `/models` listing does not filter out paused models.** A pause is a temporary action the user just took, and silently shortening the client's model list makes diagnosis harder; actually sending a request returns `route_pool.model_unavailable`, and that code is itself the answer. The cost is a brief inconsistency between the catalogue and real availability.

## What success clears

A successful forward does two things, and they are **asymmetric**:

```sql
-- Account level: the whole set of failure traces is zeroed
UPDATE route_credentials
SET transient_failure_count = 0, next_retry_at = NULL, cooldown_until = NULL,
    semantic_failure_streak_count = 0, semantic_failure_streak_fingerprint = NULL,
    last_failure_kind = NULL, last_failure_message = NULL,
    last_failure_response_json = NULL, updated_at = ?
WHERE id = ?

-- Model level: only this one model's row is deleted
DELETE FROM route_credential_models
WHERE route_credential_id = ? AND model_key = ? AND status != 'paused'
```

The account level is cleared entirely because one successful response proves both the credential and the network are fine. The model level clears **only the model this request hit, and never its siblings** — proving `glm-5.3` works says nothing about `gpt-5.6-sol`.

A `paused` row is the exception: it is not deleted, only reset to zero failures with its `paused` status intact. A success must not silently overrule a manual pause.

Note the account-level step **does not change `status`** — it only erases failure traces, because an account eligible to be selected for forwarding was already `ok`.

A successful model test does one more thing: if the account is currently `error` or `warning`, it is pulled back to `ok`; and an explicit test against a single account additionally performs full recovery (see below).

## Auto recovery

What brings a failed account back into the pool? Two routes:

1. **The backoff window expires naturally.** Once the account-level or model-level `cooldown_until` is in the past, that object is automatically a candidate again. This needs no configuration, but it only applies to accounts and models whose status is still `ok`.
2. **The auto-recovery scheduler**, for accounts already flipped to `error` or `warning`, manually `paused`, or carrying model rows. An account whose status is not `ok` will never be picked by the selection SQL, and a model flipped to `error` or `paused` will never survive partitioning, so waiting alone will not bring either back — something has to write the status back.

### Three recovery modes

The recovery rule is per account, stored in `config_json.recovery`:

```rust
pub enum RecoveryMode {
    #[default]
    Off,
    Scheduled,
    Healthcheck,
}
```

| Mode | Behaviour |
| --- | --- |
| `off` | No auto recovery (the default). Setting `off` removes the `recovery` key from config entirely |
| `scheduled` | Unconditionally reactivates the account at fixed times each day |
| `healthcheck` | Runs a real model connectivity test at a fixed interval and **recovers only if it passes** |

The scheduler ticks every **30 seconds** (`RECOVERY_TICK_SECONDS = 30`), walks every non-archived account across all platforms, decides "does this need recovery?", and then acts according to its mode.

### The needs-recovery decision

```rust
fn needs_recovery(
    status: &str,
    next_retry_at: Option<&str>,
    cooldown_until: Option<&str>,
    has_model_failures: bool,
) -> bool {
    if status == "revoked" {
        return false;
    }
    status != "ok" || next_retry_at.is_some() || cooldown_until.is_some() || has_model_failures
}
```

- A `revoked` account **never participates in auto recovery**. The reactivation SQL also carries `WHERE id = ? AND status != 'revoked'` as double protection.
- Any status other than `ok`, or any lingering backoff/cooldown timestamp, counts as needing recovery. Note that even an `ok` account gets cleaned up by the recovery flow if it still carries a backoff window.
- **The last condition is new here**: an account whose own columns look perfectly healthy but which has non-`paused` model rows also counts. Without it, an account whose only problem is one parked model would never enter the recovery flow and only live traffic would ever probe it. The candidate query adds that column with an `EXISTS` subquery rather than fetching the detail rows.

### Scheduled recovery (`scheduled`)

- Times are given as `HH:MM` and evaluated in **local time**.
- They are normalized on save: zero-padded to two digits, de-duplicated, sorted. `3:00` and `03:00` are the same time.
- **At least one time is required**, otherwise the call returns `validation.recovery_times_required`.
- An invalid format returns `validation.recovery_times`.

The trigger test is "does some configured time fall in the half-open interval between the previous tick and this one", enumerating dates day by day. That handles both edge cases correctly:

- **Across midnight**: the previous tick at 23:59:50 and this one at 00:00:20 the next day will fire a configured `00:00`.
- **The machine slept for days**: previous tick on August 11 at 16:00, this one on August 13 at 10:00, with `15:00` configured — it fires (once, not once per missed day).

What runs on trigger is an **unconditional reactivation**: status written back to `ok`, every failure counter, backoff window, streak counter, and failure detail cleared, and **every non-`paused` model row for that account deleted**. This route does not verify the account is actually usable — it assumes "a night has passed, the rate limit should be over". `paused` is the user's will and a scheduled job does not overrule it: automation may only undo automation.

### Healthcheck (`healthcheck`)

- The probe interval is in minutes, defaulting to **30**, with a legal range of **1–1440** (one day). Out of range returns `validation.recovery_probe_interval`.
- The last probe time is kept in memory, timed per account ID; restarting the app restarts the clock.
- **Which model gets probed**: the one most in need of it — a model that has a row, is not `paused`, and whose cooldown has already expired, taking the oldest `updated_at`. The repository speaks upstream model keys while the model test wants a request-side name, so the key is mapped back to one of the `model_mappings` `from` values (for official and no-mapping accounts the key already *is* the requested name and is used verbatim). Only when no such row exists does it fall back to the default behaviour (the first non-fallback mapping).
- When due, it runs an **explicit model connectivity test against that account**. Success recovers the account fully via "explicit-test recovery"; failure changes nothing and it waits for the next interval.

::: tip Why the oldest `updated_at`
When the user does not name a model, the default is "the first non-fallback mapping". With per-model cooldowns that would make healthchecks probe only the first model forever — while the reason this account is in recovery may well be that its third model is broken.
:::

In other words, the only way healthcheck mode recovers an account is by **actually completing a generation request**. That is more trustworthy than scheduled recovery, at the cost of a little quota per probe and one request event in the stats.

### Explicit-test recovery covers the account, not its sibling models

Once an explicit per-account test passes, a different path runs: it clears **only the account-level columns** and leaves model rows alone — because the row for the model this test asked about was already deleted by the success-clearing step.

That distinction is deliberate. If explicit-test recovery also wiped every model row, "testing `glm-5.3`" would incidentally erase `gpt-5.6-sol`'s cooldown, claiming the upstream answered for a model it was never asked about — which is precisely the behaviour this two-layer mechanism exists to eliminate.

### Choosing between them

| Situation | Recommendation |
| --- | --- |
| The upstream resets quota on calendar days | `scheduled`, with a time shortly after the reset |
| The upstream's rate-limit window is unpredictable | `healthcheck`, at a 15–30 minute interval |
| Many accounts, and you don't want every one burning quota on probes | `healthcheck` for primaries, `scheduled` for standbys |
| You want full manual control | `off`, and click test when you need to |

::: tip Manual testing is the fastest recovery there is
Click one model connectivity test on a single account; success performs full recovery. `paused` accounts can be tested too — the code comments on this explicitly: an explicit test is exactly how a user determines whether a paused account has come back. To clear one model's cooldown or error, 解除 ("clear") in the edit drawer's 模型状态 section is more direct and costs no quota.
:::

### What happens if config gets corrupted

If an account's `config_json` is not a valid JSON object, setting a recovery rule returns `validation.recovery_config_json` and **does not overwrite the existing config**. That is intentional: better to refuse the write than to clobber config a user hand-edited.

The read side is forgiving: a `recovery` key that fails to parse, or content that doesn't match the schema, falls back to `off` rather than letting one bad config break the entire recovery loop.

## The full timeline of one outage

Suppose a primary account maps both `gpt-5.6-sol` and `glm-5.3`, and the upstream returns 429 only for `gpt-5.6-sol`:

```text
T+0s     Request #1 for gpt-5.6-sol gets 429 -> same-account retry (200 ms) -> still 429
         -> retries exhausted -> one model-scoped failure recorded
         -> the upstream-sol row in route_credential_models cools until T+10s
            (this account has cooldown on, at the default 10 seconds)
         -> at the account level only transient_failure_count increments; no account cooldown
         -> the UI shows "1 error" and "模型 1 不可用"; the row's 冷却 N 秒 badge stays away
         -> switch to the next account in the same priority group; the client gets a normal response

T+2s     A glm-5.3 request -> the primary is selected as usual -> succeeds
         -> account-level failure traces cleared; the upstream-glm row deleted (there was none)
         -> upstream-sol's cooldown is untouched: this success answered nothing about it

T+10s    upstream-sol's cooldown expires and the primary may serve that model again
         -> 429 again -> model-scoped failure #2 -> that row cools until T+20s

T+20s    Try gpt-5.6-sol again -> this time it succeeds
         -> the upstream-sol row is deleted and account-level failure traces are zeroed
         -> the badge disappears; the primary is fully recovered for both models
```

If the upstream is down wholesale instead, each model fails once and the second failure triggers the escalation: an account-level `cooldown_until` is written and the whole account backs off — so a completely broken relay is not re-probed once per model forever.

Throughout, **the client perceives no failure at all** — every backoff came with an account switch, so long as the pool still had another usable account. That is the whole point of a multi-account pool with two cooldown layers.

## Next

- [Accounts and the Pool](/en/guide/accounts) — the status machine, priority, and concurrency limits
- [Model Connectivity Tests](/en/guide/model-test) — the same test logic behind manual recovery and healthchecks
- [Usage and Request Stats](/en/guide/usage-stats) — failed requests are recorded too, and how to find them
- [Protocol Routing and Bridging](/en/guide/protocol-routing) — which step of the forwarding chain a failure occurred in
