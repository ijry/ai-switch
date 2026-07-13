# AI Switch Provider Route Proxy Design

## Goal

Build the first real ccswitch-style routing slice: AI Switch runs a local HTTP proxy, writes Codex/Claude/Gemini CLI config files to point at it, forwards requests to configured provider API endpoints, and records usage events.

## Scope

- Provider API routing only in this slice.
- Route pool UI remains the account-selection surface, but real outbound credentials come from `providers` for now.
- Supported target families: `codex`, `claude`, and `gemini`.
- Local proxy listens on `127.0.0.1` and returns its selected port/base URL to the UI.
- Config writes are explicit user actions and use the existing atomic `ConfigWriter`.
- Usage logging records request count immediately and best-effort token/cost fields from JSON responses when available.

## Architecture

Add a Rust `route_proxy` module with a managed runtime stored in `AppState`. `start_route_proxy` binds a local port and spawns an Axum server. Each incoming request is mapped to a platform from path/header hints, a provider is selected from SQLite, and the request is forwarded with provider base URL and authorization metadata.

Add target config rendering helpers that produce conservative files:

- Codex: `~/.codex/config.toml` points a `model_provider` entry to the local proxy base URL.
- Claude: `~/.claude/settings.json` records AI Switch routing metadata and proxy environment keys for Claude-compatible clients.
- Gemini: `~/.gemini/settings.json` records AI Switch routing metadata and proxy base URL metadata.

## Data Flow

1. User starts the proxy from Accounts/Router UI.
2. UI calls `write_route_proxy_configs`.
3. Config files point apps at `http://127.0.0.1:<port>`.
4. Codex/Claude/Gemini sends requests to the proxy.
5. Proxy chooses the first enabled provider matching platform metadata, falling back to the first enabled provider.
6. Proxy forwards request path/body/headers to provider base URL.
7. Proxy inserts `usage_events` rows for request count and token estimates parsed from response JSON.

## Error Handling

- Starting an already-running proxy returns current status.
- Missing provider/base URL returns a JSON 502 response and records no provider usage.
- Config write failures return `ApiError` with filesystem/database details.
- Stop proxy aborts the spawned task and clears runtime state.

## Testing

- Unit-test provider selection and target URL construction.
- Unit-test config rendering for Codex/Claude/Gemini.
- Command-level smoke tests are covered by Rust compile/check; full network proxy tests can be added after the route adapter stabilizes.
