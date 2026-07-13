# Vibe Terminal Design

## Goal

Build a desktop-first Vibe mode for AI Switch that lets users open real in-app terminal tabs, start new agent sessions in selected project folders, and resume existing local agent sessions from the current session scanner.

## Scope

This spec covers the desktop Tauri app only. It does not expose a mobile remote-control API, WebSocket server, authentication layer, or cross-device session sharing in this pass. The backend should still use clear terminal-manager boundaries so a future mobile/Tailscale control surface can attach to the same conceptual session model without rewriting the desktop workflow.

Running terminal processes are in-memory runtime state. They are not persisted across app restart in this pass.

## User Experience

The app gets a top-level mode switch between `Agent` and `Vibe`. `Agent` keeps the existing account/provider management screens. `Vibe` opens a terminal-focused workspace with a darker, code-oriented visual treatment.

Vibe shows:

- A left sidebar with discovered local agent sessions grouped by project directory.
- A `New Session` launcher for opening an agent or blank shell in a project directory.
- A right workspace with terminal tabs.
- Each terminal tab has a title, platform label, project directory, running/exited status, and close control.

Clicking an existing session opens a new terminal tab using that session's `resumeCommand` in its `projectDir` when available. If the session lacks a usable project directory or resume command, the UI should explain the missing field instead of launching an invalid process.

Creating a new session opens a terminal tab in the chosen project directory and runs the selected agent command:

- `codex`
- `claude`
- `gemini`
- `opencode`
- `openclaw`
- `hermes`
- blank shell

## Backend Architecture

Add a runtime terminal manager to the Tauri backend. It owns PTY sessions and exposes a small command/event interface.

The first backend implementation uses `portable-pty` as the local PTY engine. `tmux` or `zellij` can be added later as optional advanced backends, but they are not required for this desktop-first pass.

The terminal manager responsibilities are:

- Create a PTY-backed process in a requested working directory.
- Run either a shell command, an agent command, or a resume command.
- Keep a registry of active terminal sessions by generated ID.
- Accept user input and write it to the PTY.
- Resize the PTY when the xterm viewport changes.
- Kill a process and remove it from the registry.
- Emit output and lifecycle events to the frontend.

Tauri commands:

- `create_terminal_session(input) -> TerminalSession`
- `write_terminal_input(session_id, data) -> ()`
- `resize_terminal(session_id, cols, rows) -> ()`
- `kill_terminal_session(session_id) -> ()`
- `list_terminal_sessions() -> Vec<TerminalSession>`

Tauri events:

- `terminal://output` with `{ sessionId, data }`
- `terminal://exit` with `{ sessionId, exitCode }`
- `terminal://error` with `{ sessionId, message }`

`AppState` should hold the terminal runtime state alongside existing runtime services.

## Frontend Architecture

Add `VibeScreen.tsx` as a new screen in the existing manual routing structure.

Add a small terminal UI component using `@xterm/xterm` and a fit addon. The component owns the xterm instance, subscribes to terminal events for one session, sends input to Tauri, and reports resize changes back to the backend.

Frontend API additions live in the existing `src/lib/api/client.ts` and `src/lib/api/types.ts` files, matching the current invoke-wrapper style.

The Vibe screen should reuse existing `listSessions(null)` data instead of adding another session scanner.

## Data Flow

Resume flow:

1. Vibe loads local sessions with `listSessions(null)`.
2. User selects a session.
3. Frontend validates that `resumeCommand` and `projectDir` are present.
4. Frontend invokes `create_terminal_session` with launch kind `resume`, command text, platform, title, and working directory.
5. Backend starts a PTY process and returns `TerminalSession`.
6. Frontend creates/selects a tab and attaches xterm to events for that session.

New session flow:

1. User selects a platform and project directory.
2. Frontend invokes `create_terminal_session` with launch kind `agent` or `shell`.
3. Backend starts the selected command in the requested directory.
4. Frontend opens a tab and streams input/output.

## Error Handling

Backend validation should reject:

- Empty working directory.
- Nonexistent working directory.
- Unsupported platform.
- Empty command.
- Input, resize, or kill requests for unknown session IDs.

Frontend should show actionable errors in the Vibe panel without crashing the screen. If a command is not installed or cannot start, the terminal tab should receive a readable error event and transition to exited/error state.

Closing a tab should kill the associated runtime session after confirmation only when the process is still running. Exited tabs can close immediately.

## Testing Strategy

Rust tests should cover command construction and validation without requiring real agent CLIs. Where PTY integration is hard to test deterministically, keep the launch-request validation and registry behavior in testable pure functions.

Frontend tests should cover:

- Vibe appears in routing/navigation.
- Sessions group by project directory.
- Missing `resumeCommand` or `projectDir` disables resume launch.
- Creating a tab calls the expected API wrapper.

Manual verification should include:

- `pnpm typecheck`
- `pnpm test:run`
- `pnpm rust:check`
- Launching a blank shell from Vibe.
- Launching `codex` or a resumable session when the CLI exists locally.

## Future Mobile Boundary

This pass intentionally avoids remote networking. The terminal manager should still keep session IDs, lifecycle events, and input/output messages independent of Tauri UI details. A later mobile API can bridge these same operations over an authenticated local WebSocket or HTTP channel exposed only on the user's trusted Tailscale interface.
