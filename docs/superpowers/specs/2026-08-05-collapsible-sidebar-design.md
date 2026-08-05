# Collapsible Sidebar Design

## Goal

Allow the desktop shell's left navigation to collapse into a compact icon rail so the account workspace gains horizontal space without losing access to navigation.

## User Experience

- The expanded sidebar remains `236px` wide and keeps the existing brand, language selector, section labels, and text navigation.
- The collapsed sidebar is about `56px` wide and shows only navigation icons.
- A three-line menu button in the sidebar header toggles expanded and collapsed states.
- Collapsed navigation icons keep accessible names and hover titles so actions remain discoverable.
- On desktop, the right content column expands automatically when the sidebar collapses.
- The account workspace title is `算力中心` when expanded and `AI Switch` when collapsed.
- Below `600px`, the shell keeps a `56px` icon rail automatically instead of expanding the sidebar.

## Architecture

- `App` owns the sidebar collapsed state so the shell and `AccountsScreen` receive the same value.
- `AppLayout` receives the state and toggle callback, switches the grid column from `236px` to `56px`, and renders the compact navigation variant.
- `NavButton` accepts an icon and collapsed flag; collapsed buttons center the icon and hide the text while preserving `aria-label` and `title`.
- The shared `AgentIcon` component renders the existing platform-specific SVG marks for agent navigation instead of generic utility icons.
- `AccountsScreen` receives the collapsed state and changes only the top-left workspace title; all account workflows remain unchanged.

## Accessibility

- The toggle exposes an accessible name and `aria-expanded` state.
- Icon-only navigation buttons retain their existing accessible labels.
- Keyboard focus styles remain visible for the toggle and every navigation action.

## Verification

- Add AppLayout coverage for expanded and collapsed sidebar states, including the toggle and accessible labels.
- Add AccountsScreen coverage for the title change when the sidebar is collapsed.
- Run `pnpm typecheck`, focused layout/account tests, and `git diff --check`.
