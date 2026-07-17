# Route Pool Request Stats Filter Design

## Summary

Add period filtering and a request list to the existing per-agent route pool statistics panel. The feature stays scoped to the active platform tab, such as Codex or Claude, and reuses the existing `usage_events` storage path instead of adding a global statistics page.

The supported filters are:

- `today`: events from the user's current local day.
- `week`: events from the current local week, Monday through now.
- `month`: events from the current local month through now.
- `all`: all recorded events.

## Goals

- Show filtered request, token, and cost totals for the current platform's enabled route pool members.
- Show a request list for the selected period.
- Keep the UI inside the existing account screen statistics panel.
- Preserve existing route pool commands and avoid adding cross-platform navigation.

## Non-Goals

- No global cross-platform statistics page.
- No custom date range picker.
- No export, pagination, or deletion workflow.
- No schema rewrite for existing `usage_events`.

## Architecture

Extend the current route pool state API rather than introducing a second frontend query. `get_route_pool` will accept an optional `since` timestamp and return `RoutePoolState` with stats already filtered from that timestamp. Existing callers that omit `since` keep cumulative behavior.

Backend changes:

- Add an optional `since` field to the route pool stats request path.
- Validate `since` as RFC3339 when present and use it as an inclusive lower-bound timestamp filter.
- Apply the filter to aggregate totals and recent log selection.
- Return request rows separately from raw metric rows so the UI can render a request list without mixing token and cost events.

Frontend changes:

- Add `statsPeriod` state to `AccountsScreen`.
- Include `statsPeriod` in the route pool query key and pass the mapped `since` timestamp to `getRoutePool`.
- Add filter buttons in the expanded statistics panel.
- Render a request list under the summary cards.

## Data Flow

1. User opens an agent tab and expands `统计`.
2. `AccountsScreen` maps the selected period to `since` and calls `get_route_pool(platform, since)`.
3. `RoutePoolService` normalizes the platform and validates `since` when present.
4. `RoutePoolRepository` queries `usage_events` joined to current enabled `route_pool_members`.
5. Aggregates include request count, token count, and cost within the selected period.
6. Request list returns `metric_type = 'request'` rows ordered by `created_at DESC`.
7. The UI renders summary cards and request rows for the active platform only.

## Request List Fields

Each list row should include:

- Created time from `created_at`.
- Account display name, falling back to the credential id when absent.
- HTTP status parsed from `metadata_json.status`, or `-` when missing.
- Request path parsed from `metadata_json.path`, or `-` when missing.
- Source label from `source_label`.

Metadata parsing remains best-effort on the frontend for display only. Invalid or missing JSON should not break rendering.

## Period Semantics

The frontend computes local period starts and sends them as ISO timestamps to avoid ambiguous server timezone assumptions. The backend treats the optional start timestamp as a lexical ISO/RFC3339 lower bound against `usage_events.created_at`.

Period behavior:

- `today`: local midnight through now.
- `week`: local Monday midnight through now.
- `month`: local first day midnight through now.
- `all`: no lower bound.

Because existing timestamps are stored as RFC3339 strings, ordering and filtering remain stable for UTC-style values already written by Rust.

## Error Handling

- Invalid platform returns the existing route pool validation error.
- Invalid period falls back to `all` on the frontend.
- Invalid `since` values are rejected by the backend with a validation error.
- Database failures use existing `AppError::Database` patterns.
- Invalid `metadata_json` is rendered as missing path/status, not as a page error.

## Testing

Backend tests:

- Route pool stats without `since` still return cumulative totals.
- `since` filters include only matching events.
- Request list contains only `request` metric rows and excludes token/cost rows.

Frontend tests:

- Statistics panel renders the four period filters.
- Selecting a period calls `getRoutePool` with the mapped `since` timestamp.
- Request rows display account, status, path, source, and timestamp.
- Invalid metadata renders fallback values instead of crashing.

## Acceptance Criteria

- Each agent tab can show request statistics scoped to that platform's route pool.
- User can switch between `当日`, `本周`, `本月`, and `累计`.
- Summary totals update with the selected filter.
- A request list appears below summary totals.
- Existing pool membership, route testing, and proxy controls keep working.
