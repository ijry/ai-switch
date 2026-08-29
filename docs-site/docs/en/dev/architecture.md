---
title: Architecture
description: How AI Switch is layered — a React UI shared by desktop and browser, the ai_switch_lib Rust core, dual Tauri IPC and axum HTTP transports, SQLite storage and the data directory layout, and a Go Tailscale sidecar.
---

# Architecture

AI Switch is built around one idea: **one body of business logic, two runtime shapes**. The same React UI runs inside a Tauri desktop shell or loads in a browser. The same Rust crate is linked into the desktop process or served by a standalone HTTP binary. Only the transport differs.

```text
┌──────────────────────────────────────────────────────────┐
│  UI layer   React 18 + TypeScript + Vite (shared)         │
└───────────────────────┬──────────────────────────────────┘
                        │  Transport abstraction (runtime probe)
        ┌───────────────┴───────────────┐
        ▼                               ▼
┌────────────────┐            ┌────────────────────────┐
│  Tauri IPC     │            │  axum HTTP + WebSocket │
│  invoke()      │            │  POST /api/:command    │
│  in-process    │            │  GET  /ws/events       │
└───────┬────────┘            └───────────┬────────────┘
        └───────────────┬─────────────────┘
                        ▼
┌──────────────────────────────────────────────────────────┐
│  Core layer   Rust crate `ai_switch_lib`                  │
│  services / models / database / mcp / skills / ...        │
└───────────────────────┬──────────────────────────────────┘
                        ▼
┌──────────────────────────────────────────────────────────┐
│  Storage   SQLite (23 migrations) + ~/.ai-switch data dir │
└──────────────────────────────────────────────────────────┘
                        │
                        ▼
┌──────────────────────────────────────────────────────────┐
│  Sidecar  ai-switch-tsnet (Go + Tailscale tsnet)          │
└──────────────────────────────────────────────────────────┘
```

## UI layer

The frontend lives in `src/` at the repository root: React 18 with TypeScript, bundled by Vite 5, styled with UnoCSS, server state managed by TanStack Query.

| Directory | Contents |
| --- | --- |
| `src/screens/` | 15 top-level screens: accounts, batches, dashboard, imports, MCP, OCR, operation log, providers, sessions, settings, skills, targets, updates, Vibe, crypto tools |
| `src/components/` | Domain-scoped components: accounts, auth, batches, brand, deeplink, imports, layout, mcp, platform, settings, skills, terminal, ui, updates, vibe |
| `src/lib/transport/` | The transport abstraction — the only place desktop and browser diverge |
| `src/lib/api/` | Command client, command-availability probing, error mapping |
| `src/lib/ocr/`, `src/lib/query/` | Local OCR and query-client configuration |
| `src/skins/` | Three built-in Vibe skins: `codex-2007-blue`, `rescue-pups-adventure-bay`, `starship-cockpit` |

Terminals are rendered with `@xterm/xterm`, Vibe skin scenes use `three`, icons come from `lucide-react`, and import/export archives are handled by `jszip`.

The UI code has no idea where it is running. `src/lib/transport/detect.ts` probes for `window.__TAURI_INTERNALS__`, and `getTransport()` hands back either a `TauriTransport` or a `WebTransport`. Every feature component talks to the same transport interface, so adding a command means writing the frontend side exactly once.

## Transport layer

### Desktop: Tauri IPC

`run()` in `src-tauri/src/lib.rs` registers **87 commands** through `tauri::generate_handler!`, covering settings, accounts and credentials, the pool, the route proxy, HTTPS certificates, sessions, target apps, terminals, the web service, Tailscale, MCP, and skills. The command bodies live in the 13 modules under `src-tauri/src/commands/` and mostly just parse arguments — the real work happens in `services/`.

The `setup()` phase also spawns several long-lived tasks: the tray menu and hide-on-close behaviour, an optional auto-start of the web service, an optional restore of the route proxy, and `RouteRecoveryService::run_loop`, which periodically re-enables accounts according to their recovery rules.

### Browser: axum HTTP + WebSocket

`src-tauri/src/web/` exposes an equivalent HTTP surface:

| Endpoint | Method | Purpose |
| --- | --- | --- |
| `/api/:command` | `POST` | Same names and arguments as the Tauri commands, dispatched by `dispatch_command` in `web/handlers/mod.rs` |
| `/ws/events` | `GET` | Event stream, the counterpart to Tauri events on desktop |
| `/health` | `GET` | Health check — the only endpoint that does not require a token |
| everything else | `GET` | Falls through to static assets, used to serve the frontend `dist` |

Security details worth knowing:

- The `authorize_api_request` middleware in `web/auth.rs` enforces bearer-token auth on `/api/*` and `/ws/events`, and it runs *before* the JSON extractor — an unauthorized request never gets its body parsed.
- **Eleven sensitive commands** (credential export, import preview, import, reading the proxy key, the four MCP write commands, and the three skill write commands) additionally pass through `gate_sensitive_commands`. When the transport does not meet the security bar they return 404 rather than 403, so the response does not even confirm the command exists.
- Request bodies are capped at 12 MiB (`SENSITIVE_COMMAND_BODY_LIMIT`).
- Every `/api/*` response carries `Cache-Control: no-store` and `Pragma: no-cache`.
- CORS allows only `GET`/`POST`/`OPTIONS` and the `Authorization`/`Content-Type` headers; preflight does not bypass auth.

`EventEmitter` in `web/event_bridge.rs` is the key abstraction for dual transports. Service code calls `emit` once; on desktop it becomes a Tauri event, in the browser it goes through `WebEventBroadcaster`'s broadcast channel (capacity 4096) and out over the WebSocket.

## Core layer: `ai_switch_lib`

`src-tauri/Cargo.toml` names the crate `ai_switch_lib`, produces `staticlib`/`cdylib`/`rlib`, and defines two binaries:

- `ai-switch` (`src/main.rs`) — the Tauri desktop app
- `ai-switch-server` (`src/bin/ai_switch_server.rs`) — the standalone HTTP server, whose `main` is a single call to `server::run_from_env()`

Top-level modules, per `src-tauri/src/lib.rs`:

| Module | Responsibility |
| --- | --- |
| `app_state` | Global state: database pool, paths, per-subsystem runtime state, terminal manager, event broadcaster |
| `commands` | Tauri command entry points (13 modules) |
| `web` | axum routing, auth, WebSocket, static assets, event bridge |
| `server` | Standalone bootstrap, environment parsing, loopback-host detection |
| `services` | The business layer — the bulk of the code |
| `models` | Domain models and serialization contracts (12 modules) |
| `database` | SQLite pool, migration runner, and 11 repositories |
| `adapters` | Per-CLI config file read/write adapters |
| `config_writer` | Safe-write primitives with snapshots and hash verification |
| `core` | The `*_core` functions shared by desktop and web (sessions, settings, terminals) |
| `mcp` | MCP server management and 11 client adapters |
| `skills` | Skill package I/O, frontmatter parsing, path validation |
| `importers` | External format importers such as example JSON |
| `session_manager` | Scans each CLI's local session files and parses messages |
| `terminal_manager` | A PTY session pool built on `portable-pty` |
| `security` | The `SecretStore` trait and an unwired keyring implementation (currently inactive) |
| `paths` | The `~/.ai-switch` directory layout |
| `error` | Unified structured `AppError` / `ApiError` |

### The services, grouped

`src-tauri/src/services/` carries most of the weight. Roughly by concern:

**Accounts and credentials**
`route_credential_service`, `route_credential_activity` (concurrency and in-flight tracking), `route_quota_service`, `route_recovery_service`, `route_credential_transfer_service`, `route_credential_transfer_codec`, `route_credential_transfer_import_service`, `batch_service`

**Routing and proxying**
`route_proxy_service` (the local entry point — 7,000+ lines covering account selection, failure classification, retries, and usage accounting), `route_pool_service`, `route_config_service`, `route_preview_service`, `route_proxy_live_log`, `route_proxy_https_service`, `route_proxy_https_trust`, `route_protocol_bridge/`

**Model capability**
`route_model_fetch_service`, `route_model_test_service`, `route_model_capability`, `codex_reasoning_cache`

**Import and interop**
`import_service`, `cpa_import_service`, `cpa_export_service`, `sub2api_import_service`, `deeplink_service`, `deeplink_protocol_service`

**Platforms and config writing**
`platform_capability_service`, `config_write_service`, `target_service`, `official_agent_identity_service`, `client_identity`

**Network and remote access**
`web_service`, `tailscale_service`, `tailscale_sidecar`, `tailscale_types`, `http_client`

**Everything else**
`settings_service`, `response_failure_service` (response-level failure and quota-exhaustion detection)

### The platform and protocol model

`src-tauri/src/models/platform.rs` defines three key enums:

- `PlatformId` — **seven platforms**: `Codex`, `Claude`, `Gemini`, `Grok`, `OpenCode`, `OpenClaw`, `Hermes`. The first four are natively supported; the last three get generic API routing only.
- `ApiDialect` — **four upstream protocols**: `openai`, `openai-responses`, `anthropic`, `gemini`. `default_api_credential_dialect()` returns `None` for OpenCode, OpenClaw, and Hermes, which is exactly why API credentials for those three must spell out a base URL and a dialect.
- `PlatformOperation` — **ten platform capabilities**: `route_credentials`, `generic_api_routing`, `config_write`, `official_import`, `official_account_routing`, `deeplink_import`, `official_quota`, `model_test`, `terminal_launch`, `session_resume`. Each is described by a `CapabilityRule` carrying availability (`Supported` / `Partial` / `Unavailable`), the credential kinds it accepts, and whether a base URL and dialect are mandatory.

Whether a given button in the UI is enabled is driven by this capability table, returned from `list_platform_capabilities` — not by hardcoded checks scattered through the frontend. See the [platform support matrix](/en/guide/platform-support).

`ProtocolBridgeKind` in `services/route_protocol_bridge/mod.rs` defines **seven bridge paths**: `ResponsesToChat`, `ResponsesToResponses`, `ResponsesToAnthropic`, `ResponsesToGemini`, `ClaudeToChat`, `ClaudeToResponses`, `ClaudeToGemini`. Each has its own request rewriter, response rewriter, and SSE streaming translation. For how this behaves in practice, see [protocol routing and bridging](/en/guide/protocol-routing).

### Two ports that are easy to confuse

| | Local route proxy | Web service |
| --- | --- | --- |
| Default address | `127.0.0.1:19527` | `127.0.0.1:3090` |
| Defined in | `DEFAULT_ROUTE_PROXY_PORT` in `services/route_proxy_service.rs` | default config in `services/web_service.rs` |
| Who connects | AI CLIs on this machine (Codex, Claude Code, …) | The AI Switch UI in a browser or on a phone |
| What flows | Model inference requests, rewritten and forwarded upstream | The app's own API calls and event stream |
| Auth | Route proxy key (written into each CLI's config) | Web access token (bearer) |

Neither depends on the other. Managing accounts from the desktop app needs no web service; watching usage stats in a browser needs no route proxy.

## Storage layer

### SQLite

`database/mod.rs` opens the pool through sqlx (max 5 connections, `foreign_keys` on). `open_migrated_pool` runs migrations at startup and, on failure, attempts repair using `backups/`.

The database file lives in `~/.ai-switch/`. Debug builds deliberately use a separate `ai-switch-dev.db` while release builds use `ai-switch.db`, so `pnpm tauri:dev` can never clobber the data of an installed release.

`src-tauri/migrations/` holds **23 migrations**, and the filenames read as a history of the product:

**Wave one (2026-07-13): the foundation**

| Migration | What it added |
| --- | --- |
| `202607130001_foundation.sql` | `target_apps`, `providers`, `official_accounts`, `batches`, `batch_items`, `import_jobs`, `target_app_states`, `config_snapshots`, `quota_snapshots`, `secure_secrets` |
| `202607130002_mcp_servers.sql` | `mcp_servers` |
| `202607130003_prompt_assets.sql` | `prompt_assets` |
| `202607130004_routing_usage.sql` | `proxy_profiles`, `failover_policies`, `usage_events`, `route_pool_members`, `route_pool_cursors` |
| `202607130005_sync_foundation.sql` | `sync_profiles`, `sync_snapshots` |
| `202607130006_sessions.sql` | `sessions`, `session_events` |
| `202607130007_updater.sql` | `update_channels`, `update_checks` |
| `202607130008_managed_instances.sql` | `managed_instances` |
| `202607130009_wakeup_tasks.sql` | `wakeup_tasks`, `wakeup_runs` |
| `202607130010_bulk_tags_plugins.sql` | `tags`, `item_tags`, `plugin_links`, `bulk_operations` |
| `202607130011_route_credentials.sql` | The `route_credentials` table, a rebuilt `route_pool_members`, and a credential link on `usage_events` |

**Wave two: the route proxy and quotas**

| Migration | What it added |
| --- | --- |
| `202607210001_route_proxy_keys.sql` | `route_proxy_keys` (local entry-point access keys) |
| `202607220001_route_credential_quota.sql` | `subscription_type`, `quota_remaining/limit/used`, `quota_updated_at` |
| `202607220002_route_credential_quota_windows.sql` | `primary_remain`, `weekly_remain`, `reset_primary`, `reset_weekly` (multiple quota windows) |
| `202607300001_route_credential_retry.sql` | `transient_failure_count`, `next_retry_at`, `cooldown_until`, `last_failure_kind/message` |

**Wave three: safe writes, migration, and richer stats**

| Migration | What it added |
| --- | --- |
| `202608010001_platform_capabilities_safe_writes.sql` | `platform` on `target_apps` and `config_snapshots`; operation-group, source-snapshot, original-file-existed, and metadata columns on snapshots |
| `202608040001_route_credential_transfer.sql` | `transfer_installation_identity`, `route_credential_transfer_origins` (provenance for cross-device transfer) |
| `202608050001_route_credential_archive.sql` | `archived_at` (archive instead of delete) |
| `202608060001_route_proxy_key_aliases.sql` | `route_proxy_key_aliases` |
| `202608060002_route_usage_breakdown.sql` | Input/output/cache token counts plus USD and CNY pricing columns on `usage_events` |

**Wave four: finer-grained failure handling and scheduling**

| Migration | What it added |
| --- | --- |
| `202608080001_route_credential_failure_response.sql` | `last_failure_response_json` (keeps the raw upstream error body) |
| `202608080002_route_credential_priority_concurrency.sql` | `route_priority` (1–5, **default 3**, with a `CHECK` constraint) and `max_concurrency` (column default 1; account creation binds `DEFAULT_ROUTE_CREDENTIAL_MAX_CONCURRENCY` = **5**) |
| `202608130001_route_credential_semantic_failure_streak.sql` | `semantic_failure_streak_count`, `semantic_failure_streak_fingerprint` |

For what this scheduling behaviour looks like from the outside, see [accounts and the pool](/en/guide/accounts) and [reliability and auto recovery](/en/guide/reliability). For how the usage columns are surfaced, see [usage and request stats](/en/guide/usage-stats).

### Where secrets live

Route credential secrets are stored in the SQLite column `route_credentials.secret_payload_json` (migration `202607130011_route_credentials.sql`), read and written directly by `database/repositories/route_credential_repository.rs`, with **no** additional encryption layer.

`security/mod.rs` does define a `SecretStore` trait and a `KeyringSecretStore` backed by the `keyring` crate — but that file is marked `#![allow(dead_code)]` and `KeyringSecretStore` is never constructed or called anywhere in the repository. It is unwired scaffolding, **not** the current storage path, and the same goes for the `keyring` dependency in `Cargo.toml`.

::: warning
In the current version, API keys sit in the local database as plaintext JSON. Treat the whole `~/.ai-switch` directory as a credential directory: mind its file permissions, back it up as sensitive data, and don't copy it into shared locations.
:::

### Directory layout

`AppPaths` in `paths.rs` defines the data directory, rooted at `~/.ai-switch`:

```text
~/.ai-switch/
├── ai-switch.db              # release database (debug uses ai-switch-dev.db)
├── settings.json             # app settings
├── web-service.json          # web service config
├── route-proxy-https.json    # route proxy HTTPS config
├── backups/
│   └── config-snapshots/     # config-write snapshots (forced to 0700 on Unix)
├── certs/route-proxy/        # route proxy self-signed certificates
├── imports/
├── logs/
└── tailscale/                # sidecar state
```

## The sidecar: ai-switch-tsnet

`sidecar/ai-switch-tsnet/` is a standalone Go program (`go 1.24.0`, depending on `tailscale.com v1.82.5`) shipped with the desktop app as a Tauri `externalBin`. It does three things:

1. Authenticates with Tailscale using OAuth or an auth key
2. Joins the tailnet as a `tsnet` node and serves on the private network
3. Reverse-proxies remote requests to the AI Switch web service on local `127.0.0.1`

On startup the sidecar prints `CONTROL 127.0.0.1:<port>` to stdout. The Rust-side `tailscale_sidecar` uses that to reach its loopback-only control API: `POST /control/start`, `/control/login-oauth`, `/control/stop`, `/control/logout`, and `GET /control/status`.

There are two exposure modes. `private` is reachable only inside the tailnet; `public` publishes an internet-facing HTTPS entry point through Tailscale Funnel. **Neither mode skips AI Switch's own token check** — Tailscale solves network reachability, it does not replace application auth. Setup steps are in [remote access and HTTPS](/en/deploy/remote-access).

## Key dependencies

Versions below come from `src-tauri/Cargo.toml`:

| Dependency | Version | Used for |
| --- | --- | --- |
| `tauri` | 2 (with `tray-icon`) | Desktop shell and IPC |
| `axum` | 0.7 (with `ws`) | HTTP layer for both the web service and the route proxy |
| `axum-server` | 0.7 (`tls-rustls-no-provider`) | HTTPS listener |
| `tokio` | 1 (macros, rt-multi-thread, fs, io-util, net, sync, process, time) | Async runtime |
| `sqlx` | 0.8 (sqlite, migrate, macros, chrono, uuid, json; runtime-tokio-rustls) | Database access and migrations |
| `reqwest` | 0.12 (rustls-tls, json, system-proxy, cookies) | Upstream HTTP client |
| `rustls` | 0.23 (`ring` provider) | TLS |
| `keyring` | 3 | Declared but not wired up — see "Where secrets live" above |
| `portable-pty` | 0.8 | Cross-platform PTY |
| `rcgen` / `x509-parser` | 0.13 / 0.16 | Route proxy self-signed certificate generation and parsing |
| `ed25519-dalek` | 2 (`pkcs8`) | Signature verification |
| `serde` / `serde_json` / `serde_yaml` / `toml` / `toml_edit` | 1 / 1 / 0.9 / 0.8 / 0.20.2 | Reading and writing each CLI's config format |
| `directories` | 5 | User directory resolution |
| `sha2` / `sha1` | 0.10 / 0.10 | Config file hashing and verification |
| `chrono` / `uuid` / `url` / `base64` / `tempfile` | 0.4 / 1 / 2 / 0.22 / 3 | General utilities |
| `tower-http` | 0.6 (`cors`) | CORS middleware |
| `tauri-plugin-{shell,process,updater,dialog,deep-link,single-instance}` | 2 | System integration |

Windows adds `windows-sys 0.61` and `winreg 0.52` for filesystem flags and protocol registration. Frontend versions come from the root `package.json`: React 18.3, TypeScript 5.5, Vite 5.4, Vitest 2.0, UnoCSS 66.7, three 0.185, `@xterm/xterm` 6.0.

## See also

- [Local setup](/en/dev/local-setup) — the commands and toolchain needed to run all of this
- [Release process](/en/dev/release) — how CI turns it into installers for three platforms
- [Web service mode](/en/deploy/web-service) and [standalone server](/en/deploy/standalone-server) — how the two transports differ in deployment
- [FAQ](/en/faq)
