# Sidebar Resize Design

## Goal

Allow the desktop navigation sidebar to be resized by dragging its right edge, using the same pointer-based interaction model as the shared `otools-git` utility. The default expanded width becomes 216px, 20px narrower than the current 236px layout.

## Behavior

- Expanded desktop width defaults to 216px and is clamped between 180px and 320px.
- A 6px resize hit area sits on the sidebar's right edge while the sidebar is expanded and the viewport is at least 600px wide.
- Pointer capture, document-level movement, global resize cursor, cancellation, and unmount cleanup are handled by a reusable React `useDragResize` hook.
- The current width is stored in `localStorage` and restored on the next mount after clamping.
- The existing collapse button remains the explicit way to switch between expanded and 56px icon-only states. The existing responsive behavior below 600px remains unchanged and does not expose the resize handle.
- Dragging never changes collapse state; it only changes the expanded width.

## Implementation

- Add `src/lib/useDragResize.ts`, adapting the external utility's axis/min/max and pointer lifecycle API to React.
- Update `AppLayout` to own the width preference, use an inline grid template for the dynamic column, and render an accessible resize handle with `aria-label` and `data-testid`.
- Keep the current nav content, collapse semantics, and mobile classes intact.

## Verification

- Add focused `AppLayout` tests for the default grid width, pointer drag updates, min/max clamping, and persisted width restoration.
- Run the focused test, typecheck, production build, and `git diff --check`.
