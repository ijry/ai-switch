# Route Pool Header Layout Design

## Summary

Refine the route pool header in `AccountsScreen` so the status area is easier to scan and no longer crowds the first row.

The layout becomes:

- Left: fixed route-pool icon.
- Middle: two-line content stack.
  - First line: `算力池` and `已加入 N 个账号`.
  - Second line: plain text `本地代理：...` and `最近路由到：...` when available.
- Right: existing action buttons.

## Goals

- Reduce horizontal crowding in the route pool header.
- Keep the existing visual container and action buttons.
- Make proxy and recent-route status plain text without pill/background styling.
- Preserve existing proxy, config-writing, test-route, and statistics actions.

## Non-Goals

- No behavior changes.
- No new data fields.
- No redesign of the statistics panel.

## Frontend Design

Update only the route pool header markup in `src/screens/AccountsScreen.tsx`.

The outer green-tinted route pool container remains. Inside it, use a three-part horizontal layout:

- Icon: fixed width, unchanged Lucide key icon.
- Content: `min-w-0 flex-1` vertical stack.
- Actions: existing button row, shrink-wrapped on the right.

The status line uses small gray text and should wrap on narrow screens if needed. Long proxy URLs and account names should remain safe through truncation or wrapping without overlapping the action buttons.

## Testing

Update `tests/AccountsScreen.test.tsx` only if necessary. Existing tests should continue to find:

- `本地代理：未启动`
- `本地代理：http://127.0.0.1:43111`
- `最近路由到：Team Account`

## Acceptance Criteria

- The route-pool icon is the leftmost item.
- The middle area has two lines: pool/member info first, proxy/recent-route status second.
- Proxy and recent-route status are plain text, not background pills.
- Existing route pool controls continue to work.
