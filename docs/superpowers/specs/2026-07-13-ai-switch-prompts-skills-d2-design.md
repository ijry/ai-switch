# AI Switch Prompts And Skills D2 Design

## Context

D1 added local MCP server metadata management. The next Phase D slice is prompts and skills management. D2 creates a local library for reusable prompt text and skill instructions that later target adapters can export or render.

D2 does not execute skills, install packages, import deep links, or write target app config.

## Goals

- Add persistent prompt/skill library records.
- Support two item types: `prompt` and `skill`.
- Store name, description, body, tags, metadata, and enabled state.
- Validate JSON fields before persistence.
- Add backend commands and a frontend library screen for list, create, and enable/disable.

## Non-Goals

- No skill execution.
- No external package installation.
- No network calls.
- No target app config rendering.
- No deep-link imports.
- No raw secret storage in metadata fields.

## Data Model

`prompt_assets` stores:

- `item_type`: `prompt` or `skill`
- `name`
- `description`
- `body`
- `tags_json`: JSON string array, defaults to `[]`
- `metadata_json`: JSON object, defaults to `{}`
- `enabled`
- `status`

Sensitive metadata keys such as `token`, `api_key`, `apikey`, `password`, or `secret` require values beginning with `env://` or `secret://`.

## Completion Criteria

- Migration creates the prompt/skill table.
- Backend can create, list, and toggle prompt assets.
- Validation rejects invalid item types, missing names/bodies, malformed JSON, and unsafe metadata secrets.
- Frontend can create/list/toggle prompt and skill records.
- Rust and frontend tests cover the flow.
