# Route Config Tab-Scoped SK Design

## Goal

Fix route proxy config writing so the action in an agent tab only writes that agent's target config, generates an `sk-...` route proxy key during the write, and clears the success message automatically after a short delay.

## Current Behavior

The Accounts screen is already scoped by the active agent tab through `platform`, but `writeRouteProxyConfigs` only sends `baseUrl`. The backend therefore writes every hard-coded target config for Codex, Claude, and Gemini in one operation.

The Codex renderer still emits `wire_api = "chat"`, which is no longer accepted by current Codex. This can produce the ACP reload error reported earlier.

The frontend stores write outcomes in state and displays them until another action clears them.

## Requirements

- The `写入配置` button in an agent tab must write only that tab's target config.
- The write request must include the active platform, normalized by the backend.
- The backend must generate an `sk-...` route proxy key as part of the write outcome and include it in the rendered target config where applicable.
- Codex config must use `wire_api = "responses"`.
- The success/result panel must disappear automatically after 3 seconds.
- Existing desktop and web transports must keep the same behavior.

## Design

Add `platform` to the frontend API wrapper for `writeRouteProxyConfigs` and pass `activePlatform` from `AccountsScreen`. Update the Tauri command and web handler to accept the platform argument and forward it to `RouteConfigService`.

Change `RouteConfigService::write_configs` to normalize the platform and choose a single target renderer. Unsupported targets should return a validation error rather than silently writing unrelated files.

Generate one route proxy key per write using a local helper such as `generate_route_proxy_key() -> String`, with the form `sk-ai-switch-<random>`. The key is returned in `RouteConfigWriteOutcome` and rendered into target configs that support key-style authentication. Codex should receive `api_key = "<generated key>"` and `wire_api = "responses"` in its generated provider config. Claude and Gemini can include the generated key in their `aiSwitch.routeProxy.apiKey` metadata and environment variables for future proxy enforcement.

In `AccountsScreen`, after `configWriteOutcomes` becomes non-empty, start a 3-second timer that clears it. Clear the previous timer if the user writes again before it expires.

## Testing

- Frontend test: writing config from the Codex tab calls `writeRouteProxyConfigs("http://127.0.0.1:43111", "codex")`.
- Frontend test or assertion: the result panel auto-clears when timers advance past 3 seconds.
- Rust tests: Codex renderer includes `wire_api = "responses"` and an `sk-` key.
- Rust tests: route config writing normalizes/selects a single requested platform instead of all targets.

## Non-Goals

- Full route proxy authentication enforcement is not required for this change unless already wired.
- Do not add new tabs or change the left navigation.
- Do not write real external configs in tests.
