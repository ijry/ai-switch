# ai-switch

AI Switch is a Tauri-based desktop foundation for AI provider and official account switching.

Phase A includes:

- Tauri 2 + React + TypeScript app shell
- Rust backend with typed Tauri commands
- SQLite foundation schema
- Batch-first provider and account grouping
- Example JSON import into a named batch
- Settings stored in `~/.ai-switch/settings.json`
- Atomic config writer primitives
- Extension interfaces for target adapters, importers, quota providers, and secret storage

## Development

Install dependencies:

```powershell
corepack enable
pnpm install
```

Run frontend checks:

```powershell
pnpm typecheck
pnpm test:run
```

Run Rust checks:

```powershell
pnpm rust:check
pnpm rust:test
```

Run the desktop app in development mode:

```powershell
pnpm tauri:dev
```

## Clean-Room Boundary

This project may study public behavior, public documentation, and public file formats from related tools. It must not copy or translate non-commercial source code from `cockpit-tools`.
