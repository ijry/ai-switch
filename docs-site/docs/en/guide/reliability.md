---
title: Reliability and Auto Recovery
description: The exact rules behind AI Switch failure classification, exponential backoff, and cooldown windows — how accounts return to the pool, and how to configure scheduled versus healthcheck recovery.
---

# Reliability and Auto Recovery

The value of a multi-account pool is not "many accounts" — it is that **when one account breaks, traffic routes around it by itself, and when the problem passes, the account comes back by itself**. This page spells out AI Switch's failure classification, backoff durations, cooldown windows, and recovery rules. Every number here comes from the code; none of them are suggestions.

## The failure state in the database

Failure state lives in a group of columns on `route_credentials`, filled in by three migrations:

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
| `transient_failure_count` | Consecutive transient failures; picks the backoff tier |
| `next_retry_at` | The earliest moment a retry is allowed |
| `cooldown_until` | When the cooldown expires |
| `last_failure_kind` | Failure classification label, see below |
| `last_failure_message` | Failure message, stored truncated |
| `last_failure_response_json` | The upstream's raw failure response, up to **8192** characters, with `…` appended past that |
| `semantic_failure_streak_count` / `_fingerprint` | Streak length for one kind of semantic failure, and its fingerprint |

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

<!-- Maintainer note: the box below describes CURRENT behaviour, not a permanent property.
     If someone wires semantic_error_threshold into the forwarding path or the model-test path,
     this warning becomes wrong and must be deleted or rewritten.
     How to check: grep semantic_error_threshold — if it appears in non-test code in
     route_proxy_service.rs / route_model_test_service.rs, it has been wired up.
     Keep the Chinese version at guide/reliability.md in sync. -->

::: warning The threshold is currently not adjustable
An account's failure policy has a `semantic_error_threshold` field (default 10, accepts 1–1000), and creating or updating an account validates its range and persists it. But in the current code, **neither the forwarding path nor the model-test path reads that value**: the only place using the streak mechanism is the quota-exhaustion channel, which hardcodes the threshold to 1. All other semantic failures go through the transient backoff described below and never touch the streak counter. So the field is presently only stored — it does not affect behaviour.
:::

## Outbound timeouts: turning a stalled upstream into a failure

Every classification above assumes the upstream eventually **produces** something — a status code or an error. But there is a third way for it to behave: **stalling**. The TCP connection is established, and then not a single byte arrives. No error, no close.

With no deadline, forwarding never receives an `Err` in that case, so same-account retry, failover and backoff all fail to happen and the client hangs indefinitely. Outbound requests therefore carry two ceilings:

| Ceiling | Value | What it bounds |
| --- | --- | --- |
| Connect timeout | 20 seconds | The TCP/TLS handshake. If it cannot connect, there is nothing to wait for |
| Read timeout | 180 seconds | The maximum gap between successful reads, **reset after each read** |

When one fires it becomes an ordinary transport-layer transient failure: logged as `transport`, then same-account retry or failover according to `failure_policy`, then the 30/120/600-second backoff ladder. The failure message names which ceiling fired and how it was configured, so a stall is distinguishable from a refused connection.

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

Once bytes are with the client they cannot be recalled, so a failure exposed after that point can no longer trigger failover. Account health is still charged, though: if the stream ends having sent data frames but never a terminal event (`response.completed` / `[DONE]` / `message_stop` / `finish_reason: stop` / `finishReason: STOP`), it is recorded as a `semantic_response_transient` failure and enters the backoff ladder.

The terminal markers are byte-identical to the buffered path's `stream_disconnected_before_completion`, so the two paths never reach different verdicts. Only the disposition differs: the buffered path can retry, the streaming path can only record — and recording is what makes the next selection avoid a chronically truncating upstream.

::: tip Usage is not lost to streaming
Tokens and cost are settled when the stream ends, including when the client disconnects early — a half-delivered response still counts toward the stats rather than vanishing.
:::

## The exact backoff and cooldown rules

Handling a transient failure is a very short piece of code, worth reading as written:

```rust
let failure_count = current.saturating_add(1);
let base_seconds = match failure_count {
    1 => 30,
    2 => 120,
    _ => 600,
};
let jitter_seconds = jitter_seconds(id, failure_count, base_seconds);
let retry_at = Utc::now() + chrono::Duration::seconds(jitter_seconds);
let cooldown_until = if failure_count >= 3 { Some(retry_at.clone()) } else { None };
```

| Consecutive transient failure | Base backoff | Cooldown set? |
| --- | --- | --- |
| 1st | 30 seconds | No |
| 2nd | 120 seconds (2 minutes) | No |
| 3rd and beyond | 600 seconds (10 minutes) | **Yes**, `cooldown_until` equals `next_retry_at` |

Three things to note:

- **The ladder tops out at the 3rd failure** — it does not double forever. An account that keeps failing is retried roughly every 10 minutes.
- **The first two failures set only `next_retry_at`, not `cooldown_until`.** Both fields behave identically for scheduling (each must be expired for the account to be usable), but only `cooldown_until` renders as "cooling" in the UI.
- **Every transient failure clears the semantic streak counter** (`semantic_failure_streak_count = 0`, fingerprint nulled). The two counters never stack.

### The jitter is deterministic

Backoff is not exactly 30/120/600 seconds; it is multiplied by a factor between 80% and 120%:

```rust
fn jitter_seconds(id: &str, failure_count: i64, base_seconds: i64) -> i64 {
    let seed = id.bytes().fold(failure_count as u64, |value, byte| {
        value.wrapping_mul(31).wrapping_add(byte as u64)
    });
    let jitter_percent = 80 + (seed % 41) as i64;
    (base_seconds * jitter_percent / 100).max(1)
}
```

The seed is computed from **the account ID plus the failure count** — no random numbers. That gives two properties:

- **Different accounts get different backoff durations**, even if they fail in the same second. This avoids the thundering herd of "the whole pool cools down together and thaws together", making it far less likely the upstream gets slammed all at once.
- **The same account at the same tier produces a reproducible duration**, which makes debugging and testing tractable.

The actual ranges: 24–36 seconds on the 1st failure, 96–144 seconds on the 2nd, 480–720 seconds from the 3rd on.

## Failure classification labels

`last_failure_kind` records which step of the chain the failure occurred in, and it is used both in the UI and while debugging:

| Label | When it fires |
| --- | --- |
| `refresh` | Refreshing an official credential's access token failed (and was judged transient) |
| `request_build` | Building the upstream request failed (bridging, auth assembly, and so on) |
| `transport` | The request could not be sent, or the upstream stalled (connection, DNS, TLS, 20s connect timeout, 180s read timeout) |
| `response_read` | The request went out but the response body could not be read to completion |
| `response_transform` | The upstream responded but the bridge's reverse conversion failed |
| `upstream_status` | The upstream returned a retryable non-2xx status |
| `semantic_response_transient` | Semantic failure, taking the transient-backoff route |
| `semantic_response_failed` | Semantic failure, taking the fingerprint-streak route (the quota-exhaustion channel) |
| `model_test_status` | A model test received a non-2xx status |
| `model_test` | Any other retryable model-test failure |

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

## How cooling accounts get skipped

The availability check during scheduling requires that `next_retry_at` **and** `cooldown_until` have both expired (an empty value counts as expired):

```rust
pub fn credential_is_retryable_now(
    next_retry_at: Option<&str>,
    cooldown_until: Option<&str>,
    now: DateTime<Utc>,
) -> bool { /* both timestamps must be <= now */ }
```

The selection SQL itself already excludes anything other than `status = 'ok'`, so accounts flipped to `error` or `revoked` never enter the candidate set at all.

### The fallback when the whole pool is cooling

If filtering leaves **no usable account whatsoever**, the scheduler does not simply fail the request — it **picks the single soonest-recovering cooling account** and tries that:

```rust
cooling.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
Ok(cooling.into_iter().take(1).map(|(_, _, credential)| credential).collect())
```

The sort key is "earliest recovery time", with a stable tiebreak on the original order. This is a deliberate trade-off: better to try one possibly-still-cooling account than to hand the client an error, because the backoff duration is only an estimate and the upstream may already have recovered.

## What success clears

A successful forward invokes the clearing logic, zeroing these fields in one statement:

```sql
UPDATE route_credentials
SET transient_failure_count = 0, next_retry_at = NULL, cooldown_until = NULL,
    semantic_failure_streak_count = 0, semantic_failure_streak_fingerprint = NULL,
    last_failure_kind = NULL, last_failure_message = NULL,
    last_failure_response_json = NULL, updated_at = ?
WHERE id = ?
```

Note this step **does not change `status`** — it only erases failure traces, because an account eligible to be selected for forwarding was already `ok`.

A successful model test does one more thing: if the account is currently `error` or `warning`, it is pulled back to `ok`; and an explicit test against a single account additionally performs full recovery (see below).

## Auto recovery

What brings a failed account back into the pool? Two routes:

1. **The backoff window expires naturally.** Once `next_retry_at` / `cooldown_until` are in the past, the account is automatically a candidate again. This needs no configuration, but it only applies to accounts whose status is still `ok`.
2. **The auto-recovery scheduler**, for accounts already flipped to `error` or `warning`, or manually `paused`. An account whose status is not `ok` will never be picked by the selection SQL, so waiting alone will not bring it back — something has to write the status back.

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
fn needs_recovery(status: &str, next_retry_at: Option<&str>, cooldown_until: Option<&str>) -> bool {
    if status == "revoked" {
        return false;
    }
    status != "ok" || next_retry_at.is_some() || cooldown_until.is_some()
}
```

- A `revoked` account **never participates in auto recovery**. The reactivation SQL also carries `WHERE id = ? AND status != 'revoked'` as double protection.
- Any status other than `ok`, or any lingering backoff/cooldown timestamp, counts as needing recovery. Note that even an `ok` account gets cleaned up by the recovery flow if it still carries a backoff window.

### Scheduled recovery (`scheduled`)

- Times are given as `HH:MM` and evaluated in **local time**.
- They are normalized on save: zero-padded to two digits, de-duplicated, sorted. `3:00` and `03:00` are the same time.
- **At least one time is required**, otherwise the call returns `validation.recovery_times_required`.
- An invalid format returns `validation.recovery_times`.

The trigger test is "does some configured time fall in the half-open interval between the previous tick and this one", enumerating dates day by day. That handles both edge cases correctly:

- **Across midnight**: the previous tick at 23:59:50 and this one at 00:00:20 the next day will fire a configured `00:00`.
- **The machine slept for days**: previous tick on August 11 at 16:00, this one on August 13 at 10:00, with `15:00` configured — it fires (once, not once per missed day).

What runs on trigger is an **unconditional reactivation**: status written back to `ok`, and every failure counter, backoff window, streak counter, and failure detail cleared. This route does not verify the account is actually usable — it assumes "a night has passed, the rate limit should be over".

### Healthcheck (`healthcheck`)

- The probe interval is in minutes, defaulting to **30**, with a legal range of **1–1440** (one day). Out of range returns `validation.recovery_probe_interval`.
- The last probe time is kept in memory, timed per account ID; restarting the app restarts the clock.
- When due, it runs an **explicit model connectivity test against that account**. Success recovers the account fully via "explicit-test recovery"; failure changes nothing and it waits for the next interval.

In other words, the only way healthcheck mode recovers an account is by **actually completing a generation request**. That is more trustworthy than scheduled recovery, at the cost of a little quota per probe and one request event in the stats.

### Choosing between them

| Situation | Recommendation |
| --- | --- |
| The upstream resets quota on calendar days | `scheduled`, with a time shortly after the reset |
| The upstream's rate-limit window is unpredictable | `healthcheck`, at a 15–30 minute interval |
| Many accounts, and you don't want every one burning quota on probes | `healthcheck` for primaries, `scheduled` for standbys |
| You want full manual control | `off`, and click test when you need to |

::: tip Manual testing is the fastest recovery there is
Click one model connectivity test on a single account; success performs full recovery. `paused` accounts can be tested too — the code comments on this explicitly: an explicit test is exactly how a user determines whether a paused account has come back.
:::

### What happens if config gets corrupted

If an account's `config_json` is not a valid JSON object, setting a recovery rule returns `validation.recovery_config_json` and **does not overwrite the existing config**. That is intentional: better to refuse the write than to clobber config a user hand-edited.

The read side is forgiving: a `recovery` key that fails to parse, or content that doesn't match the schema, falls back to `off` rather than letting one bad config break the entire recovery loop.

## The full timeline of one outage

Suppose a primary account's upstream starts returning 429:

```text
T+0s     Request #1 gets 429 → same-account retry (200 ms interval) → still 429
         → retries exhausted → transient failure #1 recorded
         → next_retry_at = T+24s ~ T+36s (no cooldown)
         → switch to the next account in the same priority group; the client gets a normal response

T+30s    Backoff expires, the primary re-enters the candidate set
         → 429 again → transient failure #2 → next_retry_at = T+126s ~ T+174s

T+150s   Try again → 429 again → transient failure #3
         → next_retry_at = cooldown_until = T+630s ~ T+870s
         → the UI shows "cooling"

T+700s   Cooldown expires, try again → this time it succeeds
         → transient_failure_count / next_retry_at / cooldown_until cleared
         → the primary is fully recovered and traffic returns to it
```

Throughout, **the client perceives no failure at all** — every backoff came with an account switch, so long as the pool still had another usable account. That is the whole point of a multi-account pool.

## Next

- [Accounts and the Pool](/en/guide/accounts) — the status machine, priority, and concurrency limits
- [Model Connectivity Tests](/en/guide/model-test) — the same test logic behind manual recovery and healthchecks
- [Usage and Request Stats](/en/guide/usage-stats) — failed requests are recorded too, and how to find them
- [Protocol Routing and Bridging](/en/guide/protocol-routing) — which step of the forwarding chain a failure occurred in
