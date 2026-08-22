# AI Switch Wakeup Tasks E2 Design

## Context

E1 added local managed instance records. The next Phase E automation item is
wakeup tasks. E2 stays conservative: it records task intent and manual run notes
without scheduling jobs, launching external tools, monitoring processes, or
calling OS wake APIs.

## Goals

- Add wakeup task records with optional managed instance, target app, and
  provider references.
- Store trigger type, schedule metadata JSON, action metadata JSON, enabled
  state, status, and notes.
- Record manual wakeup run outcomes and metadata.
- Expose Tauri commands and a `Wakeups` screen for local record management.

## Non-Goals

- No process launch.
- No PID monitoring.
- No OS task scheduler integration.
- No sleep/wake API calls.
- No target config writes.
- No raw secret storage.

## Data Model

`wakeup_tasks` stores metadata-only task definitions:

- `trigger_type`: `manual`, `scheduled`, or `interval`.
- `schedule_json`: object-shaped metadata.
- `action_json`: object-shaped metadata.
- `enabled`: boolean stored as `0` or `1`.
- `status`: `configured`, `paused`, or `error`.

`wakeup_runs` stores manual run records:

- `outcome`: `recorded`, `skipped`, or `failed`.
- `metadata_json`: object-shaped metadata.

Sensitive schedule, action, and run metadata fields must use `env://` or
`secret://` references.

## Acceptance Criteria

- Backend can create/list wakeup tasks and enable/disable tasks.
- Backend can create/list wakeup run records.
- Tauri commands expose E2 operations.
- TypeScript API exposes typed E2 wrappers.
- `Wakeups` screen can create tasks, record runs, and toggle enabled state.
- Tests cover repository, service validation, API wrappers, and screen behavior.
