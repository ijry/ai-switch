---
title: Desktop Deployment
description: The AI Switch desktop app is a Tauri 2 shell that talks to the Rust core over IPC. This page covers installer formats per platform, the local data directory layout, bundled web assets, and what the desktop shares with Web Service mode.
---

# Desktop Deployment

The desktop app is AI Switch's default form: a Tauri 2 native shell running the exact same React UI you get in a browser, minus the HTTP layer. The UI calls the Rust core (crate `ai_switch_lib`) directly over Tauri IPC. Account storage, protocol routing, the local proxy, and terminal sessions all live in that one process — there is no separate background service to start.

## Installer formats

The release pipeline builds one set of installers per platform. The formats come from the build matrix in `.github/workflows/release.yml`:

| Platform | Installer | Notes |
| --- | --- | --- |
| Windows | NSIS installer (`.exe`) | Standard install wizard; launches from the Start menu afterwards |
| macOS | `.dmg` + `.app` | The dmg is for distribution; the `.app` is the application bundle |
| Linux | `.deb` + `.AppImage` | deb targets Debian/Ubuntu family; AppImage runs without installing |

Every platform's artifacts also ship the signature files (`.sig`) and `latest.json` metadata that the Tauri updater needs to verify in-app updates. See [Release Process](/en/dev/release) for the full pipeline.

Alongside the desktop installers, each release attaches two more archives: `ai-switch-server` (the standalone server binary) and `ai-switch-tsnet` (the Tailscale sidecar). See [Standalone Server](/en/deploy/standalone-server) and [Remote Access and HTTPS](/en/deploy/remote-access) respectively.

To pick the right installer and get through first-run setup, start with [Installation](/en/guide/installation) and [Quick Start](/en/guide/quick-start).

## What happens on first launch

On first launch the app creates the data directory `~/.ai-switch` in your home directory, opens the SQLite database, and runs the bundled migrations in order (currently 23, in `src-tauri/migrations`). Migrations are idempotent, so later upgrades only apply the new ones.

Once running, the app stays in the system tray. The tray menu offers "Show Main Window" and "Quit AI Switch" — closing the window does not exit the process. That distinction matters for the local proxy, which needs to keep serving your CLIs after you close the window.

## Where the data lives

All state is local. Nothing requires a cloud account:

| Path | Purpose |
| --- | --- |
| `~/.ai-switch/settings.json` | App-level settings (language, theme, and so on) |
| `~/.ai-switch/ai-switch.db` | Main SQLite database: accounts, pool, sessions, usage, MCP, skills |
| `~/.ai-switch/ai-switch-dev.db` | Separate database used by dev builds (`pnpm tauri:dev`) so they never touch the real one |
| `~/.ai-switch/web-service.json` | Web service config (host, port, access token, Tailscale toggles) |
| `~/.ai-switch/route-proxy-https.json` | Enable/auto-start state for local pool HTTPS |
| `~/.ai-switch/certs/route-proxy/` | Self-signed root and server certificates for local pool HTTPS |
| `~/.ai-switch/tailscale/` | State directory for the Tailscale sidecar |
| `~/.ai-switch/backups/` | Backups; `backups/config-snapshots` holds snapshots taken before writing native config files (mode 0700 on Unix) |
| `~/.ai-switch/imports/` | Staging directory for import operations |
| `~/.ai-switch/logs/` | Runtime logs |

::: warning The data directory contains secrets
API keys and official-account credentials are stored in the SQLite database inside the data directory. Treat `~/.ai-switch` as a credential directory: keep it out of public repos and unencrypted sync folders, and prefer a full-disk-encrypted volume. To move your setup to another machine, use the in-app export rather than copying the database file around.
:::

## Bundled web assets

Desktop installers ship the built frontend alongside the executable, in a `web/` directory next to it. Those assets are not only for the desktop window: the moment you turn on [Web Service mode](/en/deploy/web-service) in the same app, the HTTP server serves that directory as the browser UI. Desktop users never need a separate frontend build to enable Web Service mode.

The Rust side resolves the static directory in this order: the `AI_SWITCH_STATIC_DIR` environment variable first, then candidates next to the executable (`web/`, `dist/`, `resources/web/`, and a few more), and finally `web/` and `dist/` relative to the working directory. A candidate counts as a hit only if it contains `index.html`. The same resolution logic serves the standalone server, which is why dropping a sibling `web/` folder there also works and saves you the environment variable.

## Desktop and Web Service are the same thing

Internalising this saves a lot of confusion: the desktop app and Web Service mode are not two applications. They are two ways into the same Rust core.

| | Desktop | Web Service / Standalone Server |
| --- | --- | --- |
| UI | Same React UI | Same React UI |
| Transport | Tauri IPC | `POST /api/:command` + `GET /ws/events` |
| Auth | Enforced by the local process boundary | Access token (Bearer header, or query param for WebSocket) |
| Database | The same SQLite file under `~/.ai-switch` | The same file, when running on the same machine |
| Local proxy | Listens on `127.0.0.1:19527` by default | Identical behaviour; walks upward if the port is taken |

Two practical consequences follow:

- **On one machine, desktop and Web Service read and write the same data.** An account you add in the browser shows up in the desktop app on refresh, and vice versa. There is no second copy of the config to keep in sync.
- **Proxy behaviour does not depend on the entry point.** Routing decisions, protocol bridging, retries, and auto-recovery all run the same code whether a request originated in the desktop window or a browser tab. See [Protocol Routing and Bridging](/en/guide/protocol-routing) and [Reliability and Auto Recovery](/en/guide/reliability).

Nearly every command works over both transports. Exactly three are desktop-only, because they need native desktop capabilities: opening the certificate directory, launching a session in your system terminal app, and exporting credentials through a native save dialog. Calling them from a browser returns a "desktop only" result.

## Auto-start

Three independent switches remember their own state and take effect at launch as needed:

- **Web service**: when `autoStart` in `web-service.json` is true, the HTTP server comes up right after the app starts.
- **Local pool proxy**: `route-proxy-https.json` records whether the proxy was running last time; if so, launching the app restores it, including its HTTPS configuration.
- **The desktop app**: enable "Start AI Switch with the system" in App preferences to launch the desktop app when you sign in. The tray and background services remain available, while the main window starts hidden; use the tray menu to show it.

The app also keeps an auto-recovery scheduler running, which re-enables tripped accounts on the schedule you configured, with no manual intervention.

## Next steps

- To use the same config from a phone or another computer's browser, see [Web Service Mode](/en/deploy/web-service).
- To run a long-lived instance on a headless server, see [Standalone Server](/en/deploy/standalone-server).
- For access from outside your network, or HTTPS for the local proxy, see [Remote Access and HTTPS](/en/deploy/remote-access).
- To see how the IPC and HTTP transports share one command layer in code, see [Architecture](/en/dev/architecture).
