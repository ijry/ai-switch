# ai-switch

AI Switch is a Tauri-based desktop foundation for AI provider and official account switching.

Phase A includes:

- Tauri 2 + React + TypeScript app shell
- Rust backend with typed Tauri commands
- SQLite foundation schema
- Batch-first provider and account grouping
- Example JSON import into a named batch
- Settings stored in `~/.ai-switch/settings.json`
- Atomic config writer primitives
- Extension interfaces for target adapters, importers, quota providers, and secret storage

## Development

Install dependencies:

```powershell
corepack enable
pnpm install
```

Run frontend checks:

```powershell
pnpm typecheck
pnpm test:run
```

Run Rust checks:

```powershell
pnpm rust:check
pnpm rust:test
```

Run the desktop app in development mode:

```powershell
pnpm tauri:dev
```

## Release Builds

Run the full local release verification and no-bundle Tauri build:

```powershell
pnpm release:build
```

This runs release readiness checks, TypeScript checks, frontend tests, Rust
checks, Rust tests, and `tauri build --ci --no-bundle`. The built executable is
written to:

```text
src-tauri/target/release/ai-switch.exe
```

Build unsigned Windows installers only after the same verification passes:

```powershell
pnpm release:bundle:windows
```

The Windows bundle command intentionally uses `--no-sign` until code-signing
credentials are configured. Replace `src-tauri/icons/icon.ico` with a real app
icon before public distribution.

## Clean-Room Boundary

This project may study public behavior, public documentation, and public file formats from related tools. It must not copy or translate non-commercial source code from `cockpit-tools`.

## Provider Switching B1

Provider switching B1 writes sandbox target configs only. It does not write real Claude, Codex, Gemini, OpenCode, OpenClaw, or Hermes configuration files.

Sandbox output path:

```text
~/.ai-switch/targets/<target_key>/provider.json
```

Verification flow:

1. Import or create a provider.
2. Open `Providers`.
3. Select a target such as `Codex`.
4. Click `Switch in sandbox`.
5. Open `Targets`.
6. Confirm the target shows the active provider, write status, and sandbox output path.

## Provider Switching B2.1: Codex Real Mode

B2.1 adds explicit real provider switching for Codex only. Sandbox switching remains available for all supported targets.

Codex real mode writes:

```text
<CODEX_HOME>/config.toml
```

If `CODEX_HOME` is not set, the app uses:

```text
~/.codex/config.toml
```

The Codex config contains provider metadata such as `model_provider`, `base_url`, `wire_api`, and `env_key`. It does not store raw API keys.

Safe smoke test:

1. Set `CODEX_HOME` to a temporary directory.
2. Start the app with `pnpm tauri:dev`.
3. Import or create a provider with `base_url`.
4. Open `Providers`.
5. Select `Codex`.
6. Click `Switch Codex config`.
7. Verify `<CODEX_HOME>/config.toml` contains `model_provider` and `[model_providers.ai_switch_<id>]`.
8. Verify your real `~/.codex/config.toml` was not modified when using temporary `CODEX_HOME`.

## Provider Switching B2.2: OpenCode Real Mode

B2.2 adds explicit real provider switching for OpenCode. Codex real mode and sandbox switching remain available.

OpenCode real mode writes the path from `OPENCODE_CONFIG` when set. Otherwise it writes:

```text
~/.config/opencode/opencode.json
```

The OpenCode config contains a custom OpenAI-compatible provider under `provider`, sets the top-level `model`, and stores the API key as an environment reference such as `{env:OPENAI_API_KEY}`. It does not store raw API keys.

Safe smoke test:

1. Set `OPENCODE_CONFIG` to a temporary JSON path.
2. Start the app with `pnpm tauri:dev`.
3. Import or create a provider with `base_url` and a model in `model_config_json.default`.
4. Open `Providers`.
5. Select `OpenCode`.
6. Click `Switch OpenCode config`.
7. Verify the temporary `opencode.json` contains `$schema`, `model`, `provider.<ai-switch-id>.options.baseURL`, and `provider.<ai-switch-id>.options.apiKey`.
8. Verify your real `~/.config/opencode/opencode.json` was not modified when using temporary `OPENCODE_CONFIG`.

## Provider Switching B2.3: Gemini CLI Real Mode

B2.3 adds explicit real provider switching for Gemini CLI. Codex real mode, OpenCode real mode, and sandbox switching remain available.

Gemini CLI real mode writes the path from `GEMINI_CLI_SETTINGS` when set. Otherwise it writes:

```text
~/.gemini/settings.json
```

The Gemini CLI settings file contains the selected model under `model.name` and AI Switch metadata under `aiSwitch.activeProvider`. It does not store raw API keys or provider `secret_ref` values.

Safe smoke test:

1. Set `GEMINI_CLI_SETTINGS` to a temporary JSON path.
2. Start the app with `pnpm tauri:dev`.
3. Import or create a provider with a model in `model_config_json.default`.
4. Open `Providers`.
5. Select `Gemini CLI`.
6. Click `Switch Gemini CLI config`.
7. Verify the temporary `settings.json` contains `model.name` and `aiSwitch.activeProvider`.
8. Verify your real `~/.gemini/settings.json` was not modified when using temporary `GEMINI_CLI_SETTINGS`.

## Provider Switching B2.4: Claude Code Real Mode

B2.4 adds explicit real provider switching for Claude Code. Codex, Gemini CLI, OpenCode, and sandbox switching remain available.

Claude Code real mode writes:

```text
<CLAUDE_CONFIG_DIR>/settings.json
```

If `CLAUDE_CONFIG_DIR` is not set, the app uses:

```text
~/.claude/settings.json
```

The Claude Code settings file contains provider metadata under `aiSwitch`, sets `env.ANTHROPIC_BASE_URL`, `env.ANTHROPIC_MODEL`, and optionally `env.ANTHROPIC_SMALL_FAST_MODEL`. API keys are read through `apiKeyHelper` from an environment variable such as `ANTHROPIC_API_KEY`; raw API keys are not stored.

Safe smoke test:

1. Set `CLAUDE_CONFIG_DIR` to a temporary directory.
2. Start the app with `pnpm tauri:dev`.
3. Import or create a provider with `base_url` and a model in `model_config_json.default`.
4. Open `Providers`.
5. Select `Claude Code`.
6. Click `Switch Claude Code config`.
7. Verify the temporary `settings.json` contains `env.ANTHROPIC_BASE_URL`, `env.ANTHROPIC_MODEL`, `apiKeyHelper`, and `aiSwitch.activeProvider`.
8. Verify your real `~/.claude/settings.json` was not modified when using temporary `CLAUDE_CONFIG_DIR`.

## Provider Presets And Export B3

B3 adds built-in provider presets and example JSON export. Presets create normal provider records without storing raw API keys; they use environment references such as `env://OPENAI_API_KEY`.

Preset and export smoke test:

1. Start the app with `pnpm tauri:dev`.
2. Open `Imports`.
3. Use `Provider presets` to create `OpenAI Compatible` in the default `Provider presets` batch.
4. Open `Providers` or `Batches` and confirm the provider exists.
5. Return to `Imports`.
6. Click `Export example JSON`.
7. Verify the export textarea contains `providers` and `accounts`.
8. Paste the exported JSON into the import panel with a new batch name to confirm it is re-importable.

## Tray Switching B4

B4 adds a system tray menu for quick provider switching. The tray menu includes app open, refresh, sandbox switch actions for every target, real switch actions for Claude Code, Codex, Gemini CLI, and OpenCode, and quit.

Tray smoke test:

1. Start the app with `pnpm tauri:dev`.
2. Create or import at least one provider.
3. Use the tray menu to refresh entries.
4. Choose a sandbox target switch from the tray.
5. Open `Targets` and confirm the target state updated.
6. For real mode, use temporary `CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `GEMINI_CLI_SETTINGS`, or `OPENCODE_CONFIG` paths before choosing real config actions.

## Provider Switching B5: Rollback

B5 adds rollback for successful real Claude Code/Codex/Gemini CLI/OpenCode switch snapshots. Real switches save backup metadata under the app backup directory before writing the external config. Sandbox writes do not expose rollback.

Rollback smoke test:

1. Set `CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `GEMINI_CLI_SETTINGS`, or `OPENCODE_CONFIG` to a temporary location.
2. Start the app with `pnpm tauri:dev`.
3. Switch a provider to Claude Code, Codex, Gemini CLI, or OpenCode real config.
4. Open `Targets`.
5. Click `Restore previous real config`.
6. Confirm the config file is restored to its previous content, or removed if it did not exist before the switch.

## Official Accounts C1

C1 replaces the Accounts placeholder with metadata-only official account management. Accounts can be listed, created, and optionally attached to an existing batch. The app still stores secret references only; it does not store raw tokens, perform OAuth, refresh tokens, or fetch quotas.

Account smoke test:

1. Start the app with `pnpm tauri:dev`.
2. Open `Accounts`.
3. Create an account with platform, display name, optional email/plan, metadata JSON, and optional `secret_ref`.
4. Optionally attach it to an existing batch.
5. Confirm the account appears in `Accounts`.
6. Open `Batches` and confirm the account appears under the selected batch when one was chosen.

## Official Account Quota C2

C2 adds quota snapshot cache plumbing for official accounts. It supports manually recording cached quota status, remaining labels, reset time, and JSON excerpts. This prepares the data path for future real quota providers, but C2 still performs no external quota API calls, OAuth, or token refresh.

Quota cache smoke test:

1. Start the app with `pnpm tauri:dev`.
2. Open `Accounts`.
3. Create or select an official account.
4. Click `Record quota snapshot`.
5. Enter a status, remaining label, optional reset time, and valid JSON fields.
6. Save the snapshot and confirm the account card shows the cached quota status.

## Batch Quota Health C3

C3 connects cached official account quota state to batch health. Provider child health is unchanged. Official account children are `warning` when quota is missing, `warning`, or `unknown`, and `error` when the linked quota snapshot is `error`.

Batch health smoke test:

1. Create a batch with an official account and no quota snapshot.
2. Open `Batches` and confirm the batch health is `warning`.
3. Record an account quota snapshot with status `error`.
4. Open `Batches` again and confirm the batch health is `error`.

## Official Account Import C4

C4 adds metadata-only official account bundle import for Codex, Claude, and Gemini. It accepts pasted JSON account metadata, creates official account records, attaches them to a named batch, and records an import job. It still does not parse real app credential stores, run OAuth, refresh tokens, fetch quotas, or store raw tokens/passwords/API keys.

Official account import smoke test:

1. Start the app with `pnpm tauri:dev`.
2. Open `Imports`.
3. In `Official account import`, choose `Codex`, `Claude`, or `Gemini`.
4. Paste an `accounts` JSON bundle with metadata and optional `secret_ref`.
5. Click `Import official accounts`.
6. Open `Accounts` and confirm the imported account appears.
7. Open `Batches` and confirm the account is attached to the import batch.

## Official Account Quota Refresh C5

C5 adds an explicit quota refresh action for official accounts. It uses an HTTPS JSON endpoint configured in `account_metadata_json.quota_query` and optional auth through an environment-variable name, so raw tokens are not stored in the database.

Metadata example:

```json
{
  "quota_query": {
    "endpoint_url": "https://quota.example.com/accounts/team",
    "auth_env_key": "TEAM_QUOTA_TOKEN",
    "auth_scheme": "Bearer"
  }
}
```

Expected endpoint response:

```json
{
  "status": "ok",
  "remaining_label": "80% remaining",
  "reset_at": "2026-07-14T00:00:00Z",
  "summary": { "window": "daily" }
}
```

Quota refresh smoke test:

1. Create an official account with `quota_query` metadata.
2. Set the referenced auth environment variable if needed.
3. Open `Accounts`.
4. Click `Refresh quota`.
5. Confirm the account card shows the refreshed quota snapshot.

## MCP Management D1

D1 adds local MCP server metadata management. MCP records can be listed, created, and enabled or disabled from the `MCP` screen. This prepares future target-specific MCP config rendering, but D1 does not launch MCP processes, make network calls, write external tool MCP configs, or store raw secrets.

Sensitive environment values should use references such as:

```json
{
  "BRAVE_API_KEY": "env://BRAVE_API_KEY"
}
```

MCP smoke test:

1. Start the app with `pnpm tauri:dev`.
2. Open `MCP`.
3. Create a `stdio` server with command `npx`, args JSON such as `["-y","@modelcontextprotocol/server-filesystem"]`, and environment JSON `{}`.
4. Confirm the server appears in the MCP list.
5. Disable and enable the server and confirm the status changes.

## Prompts And Skills D2

D2 adds a local prompt and skill library. Library items can be listed, created, and enabled or disabled from the `Library` screen. This prepares future exports or target-specific config rendering, but D2 does not execute skills, install packages, perform network calls, import deep links, or write external prompt/skill configs.

Metadata can store non-secret details such as owners or source labels. Sensitive metadata values must use references such as:

```json
{
  "api_key": "env://MY_TOOL_API_KEY"
}
```

Library smoke test:

1. Start the app with `pnpm tauri:dev`.
2. Open `Library`.
3. Create a `prompt` with a name, body, tags JSON such as `["review"]`, and metadata JSON `{}`.
4. Create a `skill` with reusable instructions.
5. Confirm both items appear in the Library list.
6. Disable and enable an item and confirm the status changes.

## Deep-Link Imports D3

D3 adds pasted deep-link imports on the `Imports` screen. It supports local
`ai-switch://import/...` links that carry base64url-encoded JSON payloads, then
dispatches to the existing example JSON and official account JSON importers.
D3 does not register an OS protocol handler, open external URLs, make network
calls, or execute imported content.

Supported link shapes:

```text
ai-switch://import/example_json?batch_name=Deep%20Link&source_label=shared&strategy=skip&payload=<base64url-json>
ai-switch://import/official_account_json?batch_name=Accounts&source_label=shared&platform=codex&payload=<base64url-json>
```

The route also accepts `official-account-json` and normalizes it internally.
`payload` must be UTF-8 JSON encoded with unpadded base64url.

Deep-link smoke test:

1. Start the app with `pnpm tauri:dev`.
2. Open `Imports`.
3. Paste a supported `ai-switch://import/...` link into `Deep-link import`.
4. Click `Import deep link`.
5. Open `Batches` and confirm the imported providers or accounts appear under the target batch.
6. Open `Providers` or `Accounts` and confirm imported records were created.

## Routing And Usage D4

D4 adds local routing metadata on the `Routing` screen: proxy profiles, failover
policies, and manual usage events. This is a safe foundation for later local
proxy and automation work. D4 does not start proxy processes, bind ports,
perform automatic failover, collect usage automatically, make network calls, or
sync to cloud services.

Proxy profiles accept endpoint URLs starting with `http://`, `https://`,
`socks5://`, or `socks5h://`. Proxy credentials must be stored as references
such as:

```json
{
  "auth_ref": "env://LOCAL_PROXY_AUTH"
}
```

Failover policies store ordered provider IDs as JSON, and usage events store
manual metrics with object-shaped metadata JSON.

Routing smoke test:

1. Start the app with `pnpm tauri:dev`.
2. Open `Routing`.
3. Create a proxy profile with `http://127.0.0.1:7890`.
4. Create a failover policy with provider IDs JSON such as `["provider-1","provider-2"]`.
5. Record a usage event with metric `request`, amount `1`, and unit `count`.
6. Confirm all three records appear in their lists.

## Sync Foundation D5

D5 adds a safe cloud-sync foundation on the `Sync` screen. Users can create
sync profiles for `local_folder`, `webdav`, `s3`, or `git`, then record a local
snapshot manifest with current item counts. D5 does not upload, download,
perform network calls, resolve conflicts, run background sync jobs, or store raw
sync credentials.

Sync credentials must use references such as:

```json
{
  "auth_ref": "env://WEBDAV_TOKEN"
}
```

Snapshot manifests count local providers, official accounts, MCP servers,
prompt/skill assets, proxy profiles, failover policies, and usage events.

Sync smoke test:

1. Start the app with `pnpm tauri:dev`.
2. Open `Sync`.
3. Create a WebDAV profile with endpoint `https://sync.example.com/ai-switch` and an `env://` auth reference.
4. Click `Record snapshot manifest`.
5. Confirm the snapshot appears with local item counts.

## Session Management D6

D6 adds local session records and event notes on the `Sessions` screen. Sessions
can group target, provider, official account, prompt asset, MCP server IDs,
tags, status, and notes. D6 does not launch target apps, manage multi-instance
processes, write target configs, capture transcripts, or perform network calls.

Session events can store metadata, but sensitive values must use references
such as:

```json
{
  "api_key": "env://SESSION_API_KEY"
}
```

Sessions smoke test:

1. Start the app with `pnpm tauri:dev`.
2. Open `Sessions`.
3. Create a session with a title and optional target/provider/account IDs.
4. Add a session event with type `note` and metadata `{}`.
5. Activate or archive the session from the list and confirm the status changes.

## Updater Foundation D7

D7 adds updater metadata on the `Updates` screen. Users can create update
channels and record manual update check results. D7 does not check remote
feeds, download packages, execute installers, modify the running app, or claim
automatic update support.

Feed URLs and release notes URLs must use `https://` when provided. Check
details must be object-shaped JSON.

Updates smoke test:

1. Start the app with `pnpm tauri:dev`.
2. Open `Updates`.
3. Create a `stable` channel with a HTTPS feed URL.
4. Record a manual check with current version, latest version, and status.
5. Confirm the channel and check appear in their lists.

## Multi-Instance Management E1

E1 adds local managed instance records on the `Instances` screen. Instances can
store target app IDs, provider IDs, launch argument JSON, environment references,
profile JSON, status, and notes. E1 does not launch processes, monitor PIDs,
wake tasks, write target configs, or store raw secret environment values.

Environment values that look sensitive must use references such as:

```json
{
  "API_KEY": "env://API_KEY"
}
```

Instances smoke test:

1. Start the app with `pnpm tauri:dev`.
2. Open `Instances`.
3. Create an instance with launch args JSON such as `["--profile","review"]`.
4. Confirm the instance appears in the managed instance list.
5. Mark the instance `running`, `stopped`, or `error` and confirm the status changes.

## Wakeup Tasks E2

E2 adds local wakeup task metadata and manual run records on the `Wakeups`
screen. Wakeup tasks can reference managed instances, target apps, and providers,
and can store trigger type, schedule JSON, action JSON, enabled state, status,
and notes. E2 does not schedule jobs, launch processes, monitor PIDs, call OS
wake APIs, write target configs, or store raw secret values.

Sensitive schedule, action, or run metadata fields must use references such as:

```json
{
  "api_key": "env://WAKEUP_API_KEY"
}
```

Wakeups smoke test:

1. Start the app with `pnpm tauri:dev`.
2. Open `Wakeups`.
3. Create a wakeup task with schedule JSON such as `{"window":"morning"}`.
4. Confirm the task appears in the wakeup task list.
5. Disable and enable the task and confirm the status changes.
6. Record a wakeup run and confirm it appears in the run list.

## Bulk Tags Plugins E3

E3 adds local metadata for tags, item-tag assignments, plugin links, and bulk
operation records on the `Bulk` screen. Bulk operation records capture intended
item sets and parameters; plugin links capture integration metadata. E3 does not
execute plugins, run bulk mutations, launch processes, call networks, write
target configs, or store raw secret values.

Sensitive plugin or bulk metadata fields must use references such as:

```json
{
  "api_key": "env://PLUGIN_API_KEY"
}
```

Bulk smoke test:

1. Start the app with `pnpm tauri:dev`.
2. Open `Bulk`.
3. Create a tag such as `review`.
4. Assign the tag to a local item ID.
5. Create a plugin link with config JSON such as `{"mode":"metadata"}`.
6. Create a bulk operation record with item IDs JSON such as `["provider-1"]`.
7. Confirm all records appear and disable/enable the plugin link.

## IDE Account Importers E4

E4 extends metadata-only official account JSON import to more IDE platforms:
`Cursor`, `Windsurf`, `Zed`, and `VS Code`. The existing Codex, Claude, and
Gemini import path is unchanged. E4 still requires pasted JSON metadata; it does
not read IDE credential stores, extract tokens, perform OAuth, refresh tokens,
call networks, or store raw credentials.

IDE account import smoke test:

1. Start the app with `pnpm tauri:dev`.
2. Open `Imports`.
3. In `Official account import`, choose `Cursor`, `Windsurf`, `Zed`, or `VS Code`.
4. Paste an `accounts` JSON bundle with metadata and optional `secret_ref`.
5. Click `Import official accounts`.
6. Open `Accounts` and confirm the imported account appears with the selected platform.
