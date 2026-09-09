# SaaS request logs

This module owns request-log queues and stores only. It never writes request logs to SQLite, never changes balances, and never falls back from an unavailable external driver to an in-memory/SQLite substitute.

## Parent integration

Expose `pub mod logs` from the parent SaaS module and add **`postgres`** to the existing SQLx 0.8 features. No new crate is required. All other dependencies are already in the host manifest.

`LogRuntime` is `Clone + Default`; cloning shares one worker/configuration. An unconfigured default runtime creates no files or external connections. Configure it when enabling the plugin; call `shutdown` when disabling or exiting.

```rust
pub async fn configure(&self, config: LogConfig) -> Result<(), String>;
pub async fn enqueue(&self, record: LogRecord) -> Result<(), String>;
pub async fn query(&self, query: LogQuery) -> Result<LogPage, String>;
pub async fn status(&self) -> Result<LogStatus, String>;
pub async fn shutdown(&self) -> Result<(), String>;
```

All DTO JSON fields use camelCase. `LogRecord` has string request/user/Key/group IDs, `created_at: DateTime<Utc>`, `status: u16`, signed 64-bit nonnegative duration/token/micro-dollar counters, plus schema version, UTC offset, endpoint and safe settlement/error labels. Construct records from trusted metadata, never HTTP headers, bodies, prompts, credentials or full upstream errors. Defaults capture the current local UTC offset; preserve the request's original offset when constructing delayed/historical records.

`LogQuery` defaults to the last 24 hours, page 1 and 50 rows. Its window is `[from, to)`, limited to 31 days; page size is at most 100 and the page end cannot exceed 100,000. `LogPage` exposes `items`, `total`, `page`, `pageSize`, `malformedLines`.

**User handlers must set `query.owner_user_id = Some(authenticated_user_id)` after deserialization**, or start with `LogQuery::for_user`. The owner field is skipped by Serde, so the client cannot choose it. Conflicting `userId` filters are rejected. Owner-free queries are for separately authenticated administrator handlers only. Returned records contain no internal provider account references.

`LogConfig` fields: `queue` (`memory`/`redis`), `store` (`file`/`postgres`), optional absolute `directory`, optional `redisUrl`/`postgresUrl`, stable `instanceId`, `maxRecords`, `maxBytes`, `enqueueTimeoutMs`, `shutdownTimeoutMs`, `batchSize`, `retentionDays`, `postgresMaxConnections`. Redis requires a non-default stable site-specific instance ID.

**Credential URLs deserialize but are omitted from Serialize and Debug output.** Parent configuration code must store/inject them through its secret store rather than round-tripping only the public serialized configuration. URLs and raw driver errors are never returned in status/errors. Do not expose raw submitted configuration in an HTTP response.

## Delivery and switching

- All four queue/store combinations use the same worker. Memory retains entries, including in-flight entries, until storage succeeds and ACK completes. Both queues enforce count and serialized-payload-byte limits.
- Enqueue has a finite admission deadline including external queue I/O. A full queue or unavailable service returns an error; caller cancellation/connection loss can leave the result of an external enqueue indeterminate.
- `status.accepting` is false when draining, full, or queue status is unavailable. It is a snapshot, **not an inference admission reservation**. The parent proxy must coordinate bounded in-flight inference slots and reject overload before forwarding; successful accounting must not depend on log delivery.
- Retry backoff grows from 50 ms to 5 seconds. Successful batches reset consecutive failure count. Status exposes pending count/bytes, last success, failures/retries, last safe error, corrupt-line count from the last query and shutdown remainder.
- Reconfiguration validates the new drivers before stopping old admission, then drains the old queue before starting the new writer. Existing history is not migrated. An unsuccessful drain leaves the old worker retrying and rejects new records; it does not activate the new store or discard the old queue. Retry configuration/shutdown after resolving the outage.
- Normal shutdown stops accepting and waits for drain. Timeout returns an explicit remaining count; the consumer keeps retrying while the process stays alive. Forced termination can lose all unpersisted memory-queue entries. Accounting-to-enqueue gaps are not eliminated by Redis.

## Stores and operations

File storage defaults to `home/ai-switch/logs`, not `~/.ai-switch`. Local request date/hour determines `YYYY-MM-DD/HH.log`; JSON records retain UTC timestamps and the captured offset. Appends are serialized and synced before ACK. Replayed request IDs may appear physically more than once; bounded queries deduplicate them and return newest first. Queries scan only generated date/hour paths, cap scans at 256 MiB and 250,000 unique IDs, and retain only the requested prefix of results. Narrow the window or use PostgreSQL if a scan cap is exceeded.

The root must be a dedicated absolute path without parent traversal or symlink/reparse-point ancestors. Generated paths cannot escape it. Retention only deletes canonical hourly files in expired canonical date directories, never linked paths or unrelated files. It leaves directories and unrelated contents intact. Default retention is 30 days; `null` disables cleanup. Automatic cleanup begins after approximately one minute, then runs hourly. File retention preserves the cutoff local date; PostgreSQL retention uses an exact UTC age cutoff.

Redis uses RESP2 over existing Tokio TCP, Redis Streams, a fixed consumer group/consumer scoped by `ai-switch:saas:{instanceId}:request-logs`, and atomic Lua count/byte accounting. The fixed per-instance consumer reads its pending entries before new entries after reconnect/restart. Entries are removed only after storage succeeds, using XACK/XDEL and idempotent byte-accounting removal. Do not run multiple active business instances with the same instance ID, manipulate these stream/hash/counter keys, or attach additional consumer groups. Required operations include AUTH/SELECT when configured, PING, XGROUP, XREADGROUP, EVAL and the XLEN/H* /GET/INCRBY/DECRBY/XADD/XACK/XDEL commands used by scripts. Restrict ACL key access to the instance namespace. Redis persistence/replication settings determine crash durability.

Only `redis://` is implemented; native `rediss://` is explicitly rejected. Use a trusted local connection or an authenticated TLS tunnel for encrypted Redis transport. PostgreSQL defaults to certificate-verified TLS (`verify-full`) unless the administrator explicitly supplies an `sslmode`; pass `sslmode=disable` only for an appropriate local test service.

PostgreSQL uses a separate bounded pool, statement/lock timeouts, transactional schema checks and batch insertion into `saas_request_logs`; migration metadata is `saas_log_schema`, not SQLx's shared migration table. Request ID is unique; duplicates are ignored. Indexed user/Key/time filters and a repeatable-read snapshot keep count/page consistent. All data values are bound parameters. Connection/schema/permission errors reject activation rather than silently changing drivers. A dedicated database/schema and appropriately scoped role are recommended.

## Validation

From `src-tauri` after parent wiring:

```powershell
$env:CARGO_TARGET_DIR = 'target-codex'
cargo test --locked --lib --no-default-features --features sqlx/postgres saas::logs
```

Pure unit/filesystem/lifecycle tests require no services. Ignored tests **must be run against real, disposable services**; they are not mock-driver verification. Set `SAAS_LOG_REDIS_URL` and/or `SAAS_LOG_POSTGRES_URL` in the environment, then run the applicable test:

```powershell
cargo test --locked --lib --no-default-features --features sqlx/postgres saas::logs::integration_tests::redis_recovers -- --ignored
cargo test --locked --lib --no-default-features --features sqlx/postgres saas::logs::integration_tests::postgres_commits -- --ignored
cargo test --locked --lib --no-default-features --features sqlx/postgres saas::logs::integration_tests::postgres_retention -- --ignored
cargo test --locked --lib --no-default-features --features sqlx/postgres saas::logs::integration_tests::all_four -- --ignored
```

The retention test deletes expired records from its target database, so do not point it at production. Other tests use randomized request/user/instance IDs and clean up their test records/queue keys on success. To test permission denial, additionally provide `SAAS_LOG_REDIS_DENIED_URL` and `SAAS_LOG_POSTGRES_DENIED_URL` for deliberately restricted roles and run the `external_permission_denials` ignored test. Missing environment settings fail explicitly when an ignored test is selected.
