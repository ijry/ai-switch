---
title: Installation
description: Download AI Switch desktop builds from GitHub Releases. Covers installing on Windows, macOS, and Linux, the local data directory layout, the built-in signed auto-updater, and building from source.
---

# Installation

AI Switch desktop builds come from GitHub Releases, with prebuilt packages for all three platforms. The first launch creates the data directory under your home folder; no extra configuration is needed.

## Download

Open the [latest release page](https://github.com/ijry/ai-switch/releases/latest) and find the asset for your system.

Asset names follow the pattern `ai-switch_<version>_<platform>_<original filename>`, where `<platform>` is `windows-x86_64`, `darwin-aarch64`, or `linux-x86_64`. Match on that prefix.

Versions containing `-rc`, `-beta`, or `-alpha` are prereleases. For normal use, pick one without those suffixes.

### Windows

Download the NSIS installer (`.exe`) and run it. Current releases are **x86_64**.

After installation, the directory holding the executable also contains a `web/` folder with the web UI's static assets, which web service mode serves.

### macOS

Two assets are published:

- **`.dmg`** — mount it and drag AI Switch into Applications. The usual route.
- **`.app`** (inside an archive) — unpack and run it directly.

Current releases are **aarch64 (Apple Silicon)**, because the macOS CI build runs on an Apple Silicon runner. Intel Macs need to build from source.

If Gatekeeper blocks the first launch, allow it once under System Settings → Privacy & Security.

### Linux

Two assets, both **x86_64**:

- **`.deb`** — on Debian/Ubuntu, install with `sudo apt install ./<filename>.deb`
- **`.AppImage`** — no installation; `chmod +x` and run

```bash
# .deb
sudo apt install ./ai-switch_0.6.7_linux-x86_64_ai-switch_0.6.7_amd64.deb

# AppImage
chmod +x ./ai-switch_*_linux-x86_64_*.AppImage
./ai-switch_*_linux-x86_64_*.AppImage
```

AI Switch is a Tauri app and depends on the system WebKitGTK. If your distribution doesn't ship it, install it yourself — on Debian/Ubuntu that means `libwebkit2gtk-4.1-0`, `libgtk-3-0`, and `librsvg2-2`, plus ayatana appindicator for the tray icon. The `.deb` declares its dependencies so `apt` resolves them automatically; with the AppImage you have to check manually.

## Where your data lives

All local state sits in `~/.ai-switch/` under your home directory (`%USERPROFILE%\.ai-switch\` on Windows). This path is resolved at startup and is **not configurable**.

```text
~/.ai-switch/
├── settings.json                    # application settings
├── ai-switch.db                     # SQLite database (release builds)
├── ai-switch-dev.db                 # SQLite database (dev builds, isolated from release)
├── web-service.json                 # web service configuration
├── route-proxy-https.json           # local proxy HTTPS configuration
├── backups/
│   └── config-snapshots/            # snapshots taken before writing CLI config (mode 0700 on Unix)
├── imports/                         # intermediate files from account imports
├── logs/                            # logs
├── tailscale/                       # Tailscale sidecar state
└── certs/route-proxy/               # HTTPS certificates for the local proxy
```

A few notes.

**`settings.json`** holds application-level settings. Paths such as the database location are written back into this file, but they are informational — editing them will not relocate anything.

**The SQLite database** carries the real data: route accounts, pool membership and cursors, usage events, sessions, MCP servers, skills, and more. Its schema is managed by the **23 migrations** in `src-tauri/migrations` and applied automatically at startup. Release builds use `ai-switch.db` and development builds (`tauri dev` / debug) use `ai-switch-dev.db`, so local development can't damage the data you use day to day.

If a migration conflict occurs — for instance after downgrading from a newer version — the database file is moved into `backups/` with a `.migration-conflict-<timestamp>` suffix rather than being corrupted in place.

**`backups/config-snapshots/`** is part of the safe-write mechanism. Before every change to a CLI config file, AI Switch stores a snapshot here for rollback. On Unix this directory is set to `0700`.

::: warning Treat `~/.ai-switch` as a credential directory
Route account secrets — API keys and tokens — are stored in the **SQLite database** under `~/.ai-switch`, not in the OS keychain.

Which means:

- **Do not** commit this directory or the database file to Git, drop it on a shared drive, or include it in a public backup
- Back it up at credential sensitivity, preferably encrypted
- On a shared machine, verify the directory permissions so only you can read it
- Moving to a new machine is a matter of copying the whole `~/.ai-switch` directory — it carries all state, including secrets
:::

### Files AI Switch writes elsewhere

Beyond its own data directory, AI Switch modifies CLI config files when you use the "write route config files" action:

| Platform | File |
| --- | --- |
| Codex | `~/.codex/config.toml`, plus `~/.codex/ai-switch-model-catalog.json` |
| Claude Code | `~/.claude/settings.json` |
| Gemini CLI | `~/.gemini/settings.json` |
| Grok | `~/.grok/settings.json` |

These writes are **safe direct writes**: a snapshot is taken before the change, the write is atomic, concurrent modifications are detected, and guarded rollback is supported. Your other settings in those files are preserved — AI Switch only adds or updates the fields it manages.

OpenCode, OpenClaw, and Hermes are not in this list; AI Switch does not write their native configuration.

## Auto updates

The desktop app has a built-in updater, so you don't have to watch for new versions.

**Manual checks.** There's an Updates screen in the app where you can check, download, and install, then restart when prompted.

**Automatic checks.** The app checks once after launch and hourly after that. When a new version is available it shows a prompt and you decide whether to install. The interval is fixed; there's currently no toggle or update-channel setting.

**Signature verification.** Update metadata is read from `latest.json` on GitHub Releases, and every platform asset carries a minisign signature. The signature is verified against the public key built into the app before anything is installed, and installation is refused if verification fails. The release pipeline adds a second check confirming that each signature's key id matches the public key, failing the build otherwise.

So the trust anchor for the update path is the public key compiled into the app — a package swapped in transit will not verify.

## Building from source

If you'd rather not use a prebuilt package, or you need a target that isn't published — an Intel Mac, Linux on ARM — you can build it yourself.

Broadly you'll need Node (with pnpm), a Rust toolchain, and Go for building the Tailscale sidecar:

```powershell
corepack enable
pnpm install
pnpm build
pnpm tauri:build
```

Full environment requirements, per-platform system dependencies, and the dev-mode and check commands are in [Local Setup](/en/dev/local-setup). Release and CI details are in [Release Process](/en/dev/release).

## Next steps

- [Quick Start](/en/guide/quick-start) — add an account, start the proxy, make your first request
- [Platform Support Matrix](/en/guide/platform-support) — how far support goes for your CLI
- [Desktop](/en/deploy/desktop) — desktop deployment details
- [Web Service Mode](/en/deploy/web-service) — access from a browser or phone
