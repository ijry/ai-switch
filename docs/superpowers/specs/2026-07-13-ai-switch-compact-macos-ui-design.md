# AI Switch Compact macOS UI Design

## Goal

Refresh the app shell, account management screen, and settings hub into a compact, user-facing desktop UI that feels closer to macOS utilities: calm, dense, fast to scan, and free of implementation-facing copy.

## Visual Direction

- Use a light neutral desktop surface with translucent panels, subtle borders, small radii, and restrained shadows.
- Prefer compact toolbar rows, segmented controls, and dense list rows over large marketing-style cards.
- Use amber only as a small accent for active state and primary action, not as a dominant brand block.
- Keep typography functional and system-native for desktop app legibility.

## UX Rules

- Do not show development or phase-oriented language in the interface.
- Keep user copy action-oriented: account, pool, proxy, config, status, settings.
- Preserve existing workflows and labels that tests depend on where practical.
- Keep controls accessible with visible focus states, labels for form controls, and stable hover states.

## Technical Approach

- Replace Tailwind CSS runtime integration with UnoCSS through Vite.
- Keep utility-class authoring style so the redesign stays low-risk and localized.
- Update tests only where visible copy intentionally changes.

## Scope

- Modify `package.json`, `vite.config.ts`, and `src/styles.css` for UnoCSS.
- Redesign `src/components/layout/AppLayout.tsx`.
- Redesign `src/screens/AccountsScreen.tsx`.
- Redesign `src/screens/SettingsScreen.tsx`.
- Adjust tests for copy changes without weakening behavior assertions.

## Verification

- `pnpm typecheck`
- `pnpm test:run`
- `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`
