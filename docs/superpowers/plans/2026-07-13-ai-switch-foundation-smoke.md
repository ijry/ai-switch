# AI Switch Foundation Smoke Checklist

Run these checks after completing Phase A implementation.

- `pnpm typecheck` exits with code `0`.
- `pnpm test:run` exits with code `0`.
- `pnpm rust:check` exits with code `0`.
- `pnpm rust:test` exits with code `0`.
- `pnpm tauri:dev` opens the app window.
- Navigate to `Batches`; the empty state renders.
- Navigate to `Imports`; paste `fixtures/example-import.json`, enter batch name `Manual July Batch`, and import.
- Navigate to `Batches`; the new batch appears.
- Expand the batch; `Acme Claude` and `Team Account` appear.
- Navigate to `Targets`; the seven default target apps appear.
- Navigate to `Settings`; the data directory path is visible.
