# Route Request Detail View Design

## Context

The route pool statistics panel already receives request rows from `usage_events`.
Each row includes `metadata_json`, and real proxy traffic currently records safe metadata:
platform, selected route credential id and name, request path, and upstream status.
The UI only renders a compact row with time, account, status, path, and source.

The manual "test route" action records a route-pool request event, but its metadata is
limited to `{"source":"ui_test_route"}`. That makes test-route records hard to inspect
because they do not show a path or status in the request list.

## Goal

Add a safe request detail view for route pool records.
Users can inspect what the app already knows about a route request without storing or
displaying sensitive payloads.

## Non-Goals

- Do not store request bodies, response bodies, prompts, completion text, API keys, or
  authorization headers.
- Do not change the `usage_events` schema.
- Do not add a separate detail API while the existing route pool response already carries
  the required row data.
- Do not attempt full HAR-style proxy logging in this change.

## Design

### Data

Continue using `RoutePoolUsageLog.metadata_json` as the detail payload. The backend already
validates manual route-pool metadata as JSON object data before writing it.

For test-route events, write richer safe metadata:

- `source`: `ui_test_route`
- `path`: `/__ai-switch/test-route`
- `status`: `selected`
- `request_kind`: `manual_pool_selection`

For real proxy requests, keep the existing metadata fields:

- `platform`
- `route_credential_id`
- `route_credential_name`
- `path`
- `status`

### UI

In the route pool request list, each request row becomes expandable with a compact
"详情" control. The expanded area shows:

- request id
- selected account id and name
- source label
- metric amount and unit
- created time
- parsed metadata as formatted JSON

If `metadata_json` is invalid JSON, the detail view shows the raw string with a clear
invalid-metadata label. The compact row remains usable and keeps its current fallback values.

### Privacy

The detail panel is intentionally metadata-only. It must not infer, capture, or display
request/response bodies. This keeps the feature useful for routing audits while avoiding
the common failure mode of logging private prompts and credentials.

### Errors

Invalid metadata should not break rendering. Parsing stays local to the request row and
falls back to raw text for that row only.

### Testing

Frontend tests cover:

- request rows render the existing compact status/path columns
- expanding a row displays account/source/id and formatted metadata
- invalid metadata still renders a detail panel with raw metadata
- clicking "测试路由" sends the richer safe metadata

Rust tests do not need schema changes. Existing route-pool service tests continue to cover
metadata validation and paginated request rows.
