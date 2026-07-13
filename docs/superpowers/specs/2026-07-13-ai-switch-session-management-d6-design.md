# AI Switch Session Management D6 Design

## Context

D5 added sync profile metadata and local snapshot manifests. The next Phase D item is session management. D6 adds local session records and event notes so users can group provider/account/target context without launching tools.

D6 does not start processes, open IDEs, switch providers automatically, or manage multiple instances. Those behaviors belong to Phase E automation.

## Goals

- Add session records that can reference a target app, provider, official account, prompt asset, and MCP server IDs.
- Add session event records for notes and operational breadcrumbs.
- Add status transitions for `draft`, `active`, and `archived`.
- Add a `Sessions` screen for creating sessions, changing status, and adding events.

## Non-Goals

- No process launch.
- No multi-instance management.
- No automatic target config writes.
- No cloud sync of session content.
- No transcript capture.

## Data Model

`sessions` stores title, optional target/provider/account/prompt references, MCP server IDs JSON, tags JSON, status, notes, and timestamps.

`session_events` stores session ID, event type, message, metadata JSON, and creation time.

## Safety Rules

- MCP server IDs JSON must be an array of non-empty strings.
- Tags JSON must be an array of strings.
- Event metadata JSON must be an object.
- Sensitive event metadata values must use `env://` or `secret://` references.
- Status changes are limited to `draft`, `active`, and `archived`.

## Completion Criteria

- Backend can create/list sessions, set session status, and create/list events.
- Tauri commands expose all D6 operations.
- `Sessions` screen can create records, add events, and change status.
- Tests cover success and validation paths.
