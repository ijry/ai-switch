# AI Switch MCP Management D1 Implementation Plan

**Goal:** Add local MCP server metadata management.

**Architecture:** Add an `mcp_servers` migration, model, repository, service, Tauri commands, TypeScript API wrappers, and an MCP screen. D1 manages records only; later phases can render these records into target-specific MCP config.

## Guardrails

- D1 must not launch MCP processes.
- D1 must not perform network calls.
- D1 must not write target app MCP config files.
- D1 must not store raw token, password, API key, or secret values.

## Steps

- [x] Add `mcp_servers` migration and model.
- [x] Add repository create/list/toggle helpers and tests.
- [x] Add service validation for transports, JSON fields, and secret references.
- [x] Add Tauri commands and invoke registration.
- [x] Add frontend API types/functions.
- [x] Add MCP screen for list/create/toggle.
- [x] Add frontend/API tests.
- [x] Update README D1 notes.
- [x] Run `cargo fmt`, `pnpm typecheck`, `pnpm test:run`, `pnpm rust:check`, and `pnpm rust:test`.
