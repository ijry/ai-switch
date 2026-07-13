# AI Switch MCP Management D1 Design

## Context

Phase D starts advanced `cc-switch` parity. The first useful slice is MCP server management: users can register MCP server metadata that later target adapters can render into tool-specific config files.

D1 stays local and conservative. It does not start MCP processes, connect to remote MCP servers, write target app MCP config files, or store raw secrets.

## Goals

- Add persistent MCP server records.
- Support local `stdio` servers and URL-based `sse` / `streamable_http` servers.
- Validate JSON fields before persistence.
- Reject raw secret-looking environment values unless they are stored as references.
- Add backend commands and a frontend MCP screen for list, create, and enable/disable.

## Non-Goals

- No MCP process launch or health probing.
- No external network calls.
- No target app config rendering.
- No raw token, password, API key, or secret storage.
- No deep-link import.

## Data Model

`mcp_servers` stores:

- `name`
- `transport`: `stdio`, `sse`, or `streamable_http`
- `command`: required for `stdio`
- `args_json`: JSON array, defaults to `[]`
- `url`: required for URL transports
- `env_json`: JSON object, defaults to `{}`
- `enabled`
- `notes`
- `status`

Sensitive env keys such as `token`, `api_key`, `apikey`, `password`, or `secret` require values beginning with `env://` or `secret://`.

## Completion Criteria

- Migration creates the MCP server table.
- Backend can create, list, and toggle MCP servers.
- Validation rejects missing transport requirements and unsafe env secret values.
- Frontend can create/list/toggle MCP servers.
- Rust and frontend tests cover the flow.
