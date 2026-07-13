# AI Switch Multi-Instance Management E1 Design

## Context

Phase D is complete through updater metadata. Phase E starts `cockpit-tools` style automation. The first item is multi-instance management. E1 creates a safe local foundation by storing instance configurations and manual status records.

E1 does not start processes, open IDEs, monitor PIDs, or wake sleeping tasks.

## Goals

- Add managed instance records with optional target app and provider references.
- Store launch args JSON, environment JSON, and profile metadata JSON.
- Allow manual status updates for `configured`, `running`, `stopped`, and `error`.
- Add an `Instances` screen for creating records and changing status.

## Non-Goals

- No process launching.
- No PID monitoring.
- No terminal or IDE control.
- No wakeup scheduling.
- No automatic provider switching.

## Safety Rules

- Launch args JSON must be an array of strings.
- Environment JSON must be an object.
- Sensitive environment values must use `env://` or `secret://` references.
- Profile JSON must be an object.
- Status values are limited to `configured`, `running`, `stopped`, and `error`.

## Completion Criteria

- Backend can create/list instances and set instance status.
- Tauri commands expose E1 operations.
- `Instances` screen can create instances and record status changes.
- Tests cover success and validation paths.
