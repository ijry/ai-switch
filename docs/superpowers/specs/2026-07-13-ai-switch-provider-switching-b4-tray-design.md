# AI Switch Provider Switching B4 Tray Design

## Context

B1 added sandbox provider switching. B2.1, B2.2, and B2.3 added explicit real config writes for Codex, OpenCode, and Gemini CLI. B3 added provider presets and export. B4 adds a desktop tray entry point for quick provider switching without changing the database schema or provider switch semantics.

## Goals

- Create a system tray menu during app startup.
- Expose app open, tray refresh, switch actions, and quit from the tray.
- Include sandbox switch actions for every enabled target.
- Include real switch actions only for targets with implemented real adapters.
- Reuse the existing `ProviderSwitchService` so tray switches produce the same snapshots and target state as UI switches.
- Add a Tauri command to refresh tray menu contents after providers or targets change.

## Non-Goals

- No schema migration.
- No background polling for provider or target changes.
- No tray-only provider editing or import flows.
- No rollback action from the tray; rollback remains on the Targets screen.
- No real switching for targets without implemented real adapters.
- No secret resolution or raw credential storage.

## Menu Model

The tray menu contains:

- Open AI Switch.
- One provider submenu per provider when providers exist.
- A sandbox submenu under each provider with every enabled target.
- A real config submenu under each provider with Codex, Gemini CLI, and OpenCode when those targets are enabled.
- Refresh tray menu.
- Quit.

When no providers exist, the menu shows a disabled provider placeholder instead of switch actions.

Switch menu ids use this shape:

```text
ai-switch:tray:switch:<mode>:<target_app_id>:<provider_id>
```

Supported modes are `sandbox` and `real`.

## Backend Flow

Tray setup and refresh load providers and target apps from the existing repositories. Menu item selection parses the menu id, invokes `ProviderSwitchService::switch_provider`, and then refreshes the menu.

Real-mode tray actions are limited to targets that have real adapters:

- `codex`
- `gemini_cli`
- `opencode`

All switch outcomes and failures continue to be recorded through the provider switch service. Tray event handlers log errors to stderr instead of surfacing an in-app modal.

## API

`refresh_tray_menu` returns `TrayMenuStatus`:

- `provider_count`
- `target_count`
- `real_target_count`
- `switch_item_count`

This command gives the frontend and tests a stable way to verify tray menu refresh wiring without depending on OS tray rendering.

## Testing

Rust:

- Tray switch menu ids parse valid sandbox and real actions.
- Unknown tray menu ids are rejected.
- Switch item counts include sandbox targets and supported real targets.

Frontend:

- API client invokes `refresh_tray_menu`.

## Completion Criteria

- Tray is initialized on app startup.
- Tray menu can be refreshed through a Tauri command.
- Sandbox tray actions are generated for every enabled target.
- Real tray actions are generated only for Codex, Gemini CLI, and OpenCode.
- Tray switch actions reuse existing provider switch behavior.
- README documents the B4 smoke flow.
- Rust and frontend tests pass.
