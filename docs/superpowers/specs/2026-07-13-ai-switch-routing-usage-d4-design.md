# AI Switch Routing And Usage D4 Design

## Context

D1 added MCP metadata, D2 added prompt/skill library records, and D3 added pasted deep-link imports. The next Phase D slice starts the larger local proxy, failover, and usage tracking area with a safe local foundation.

D4 manages records only. It does not start a proxy process, bind ports, rewrite target app configs, make network calls, sync to cloud storage, or collect usage automatically.

## Goals

- Add local proxy profile records for endpoints such as `http://127.0.0.1:7890` or `socks5://127.0.0.1:1080`.
- Add failover policy records with ordered provider IDs stored as JSON.
- Add manual usage event records that can reference a provider and optionally an official account.
- Add a `Routing` screen for creating and listing these records.
- Validate JSON shape and secret references before persisting records.

## Non-Goals

- No local proxy server runtime.
- No automatic provider failover execution.
- No automatic usage capture or billing calculation.
- No cloud sync, session manager, updater, or plugin linkage.

## Data Model

`proxy_profiles` stores endpoint metadata, optional auth references, enablement, and notes.

`failover_policies` stores ordered provider IDs as JSON and a strategy. D4 supports `ordered` and `round_robin` as metadata values only.

`usage_events` stores manually entered usage metrics with `metric_type`, `amount`, `unit`, optional provider/account references, and metadata JSON.

## Safety Rules

- Proxy URLs must use `http://`, `https://`, `socks5://`, or `socks5h://`.
- Proxy auth values must be references starting with `env://` or `secret://`.
- Failover provider IDs JSON must be an array of non-empty strings.
- Usage metadata must be a JSON object.
- Usage amounts must be zero or positive.

## Completion Criteria

- Backend can create/list proxy profiles, failover policies, and usage events.
- Tauri commands expose all D4 operations.
- `Routing` screen can create and list all three record types.
- Frontend and Rust tests cover success and validation paths.
- README documents D4 behavior and smoke test.
