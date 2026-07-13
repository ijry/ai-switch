# AI Switch Deep-Link Imports D3 Design

## Context

D1 added MCP metadata management and D2 added prompt/skill library management. The next Phase D item is deep-link imports. D3 adds a conservative pasted deep-link importer that reuses existing import services.

D3 does not register an OS protocol handler, open external links, fetch remote URLs, or execute imported content.

## Goals

- Add an `import_deep_link` command.
- Support `ai-switch://import/example_json` links with base64url JSON payloads.
- Support `ai-switch://import/official_account_json` links with base64url JSON payloads and explicit account platform.
- Reuse existing import job creation and batch attachment behavior.
- Add an Imports screen paste entry point.

## Non-Goals

- No OS protocol registration.
- No external browser or shell open.
- No network fetch.
- No deep-link import for MCP, prompts, or skills yet.
- No raw credential storage beyond existing import guardrails.

## Link Shape

Example JSON:

```text
ai-switch://import/example_json?batch_name=Deep%20Link&source_label=share&strategy=skip&payload=<base64url-json>
```

Official account JSON:

```text
ai-switch://import/official_account_json?batch_name=Accounts&source_label=share&platform=codex&payload=<base64url-json>
```

`payload` is UTF-8 JSON encoded with unpadded base64url. The backend enforces URL and payload size limits before dispatching to existing import services.

## Completion Criteria

- Backend parses and imports supported deep links.
- Unsupported schemes/routes and malformed payloads return stable validation errors.
- Imports screen can paste and submit deep links.
- Rust and frontend tests cover successful imports and validation paths.
