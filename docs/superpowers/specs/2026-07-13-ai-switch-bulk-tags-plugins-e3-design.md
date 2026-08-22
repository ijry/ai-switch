# AI Switch Bulk Tags Plugins E3 Design

## Context

E1 added managed instances and E2 added wakeup task metadata. The final current
Phase E automation item is bulk operations, tags, and plugin linkage. E3 creates
a safe local metadata foundation only.

E3 does not execute bulk changes, run plugins, load plugin code, launch external
processes, or write target app configs.

## Goals

- Add global tag records.
- Add item-to-tag assignment records for supported local item types.
- Add plugin link records that attach plugin metadata to local items.
- Add bulk operation records that capture intended item sets and parameters.
- Expose Tauri commands and a `Bulk` screen for local record management.

## Non-Goals

- No plugin loading or execution.
- No external process launch.
- No automatic bulk mutation of providers, accounts, target configs, or files.
- No network calls.
- No raw secret storage.

## Data Model

`tags` stores reusable labels.

`item_tags` stores tag assignments with `item_type` and `item_id` references.
The references are intentionally generic because E3 is metadata-only.

`plugin_links` stores integration metadata:

- `plugin_key`: local identifier such as `review.bridge`
- `item_type` / `item_id`: attached local item
- `config_json`: object-shaped metadata
- `enabled` and `status`

`bulk_operations` stores planned or recorded bulk intents:

- `operation_type`: `tag_apply`, `tag_remove`, `status_record`,
  `export_selection`, or `plugin_link`
- `target_type`: supported local item type or `mixed`
- `item_ids_json`: string array
- `parameters_json` and `summary_json`: object-shaped metadata
- `dry_run`: indicates record-only planning

Sensitive metadata fields must use `env://` or `secret://` references.

## Acceptance Criteria

- Backend can create/list tags and tag assignments.
- Backend can create/list plugin links and enable/disable plugin links.
- Backend can create/list bulk operation records.
- Tauri commands expose E3 operations.
- TypeScript API exposes typed E3 wrappers.
- `Bulk` screen can create records and toggle plugin links.
- Tests cover repository, service validation, API wrappers, and screen behavior.
