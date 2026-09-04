---
title: Local Setup
description: Set up an AI Switch development environment — toolchain versions, dependency installation, frontend and Rust checks, sidecar tests, running the desktop app, building installers, and working on this documentation site.
---

# Local Setup

This page lists everything needed to get AI Switch running locally. Run all commands from the repository root unless a different working directory is called out.

## Toolchain requirements

Versions follow what CI actually uses in `.github/workflows/release.yml`. Matching CI locally is the cheapest way to avoid "but it works on my machine".

| Tool | Version | Notes |
| --- | --- | --- |
| Node.js | 22 | CI pins this via `actions/setup-node` |
| pnpm | 10.12.4 | Matches the `packageManager` field in the root `package.json`; CI pins it via `pnpm/action-setup` |
| Rust | stable | CI uses `dtolnay/rust-toolchain@stable` |
| Go | stable | Only needed to build or test the sidecar |

On Windows and macOS, Tauri 2's system requirements come from the platform toolchains (MSVC build tools and WebView2 on Windows, Xcode Command Line Tools on macOS).

Linux needs extra system packages, matching the `apt-get` step in CI:

```bash
sudo apt-get update
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev \
  patchelf \
  libgtk-3-dev
```

## Install dependencies

pnpm ships through corepack, so enable it first:

```powershell
corepack enable
pnpm install
```

Rust dependencies need no separate step — the first `cargo` command fetches them.

## Frontend checks

```powershell
pnpm typecheck
pnpm test:run
```

- `pnpm typecheck` is `tsc --noEmit`: type checking only, no emitted files.
- `pnpm test:run` is `vitest run`, a single pass with no watch mode. Use `pnpm test` while developing if you want watch.

## Rust checks

```powershell
pnpm rust:check
pnpm rust:test
pnpm server:check
```

All three are thin wrappers that invoke cargo inside `src-tauri`:

- `pnpm rust:check` → `cargo check`
- `pnpm rust:test` → `cargo test`
- `pnpm server:check` → `cargo check --bin ai-switch-server`, verifying the standalone server target compiles on its own

::: tip
On Windows, `cargo check` occasionally panics in the `tauri-build` step with `PermissionDenied`. This is a transient lock on files in the build output directory — just run it again.
:::

## Sidecar tests

The Tailscale sidecar is a separate Go module, so tests need a different working directory:

```powershell
cd sidecar/ai-switch-tsnet
go test ./...
```

To build the sidecar binary on its own (it must be in place before `pnpm tauri:build` can package the desktop app):

```powershell
cd sidecar/ai-switch-tsnet
go build -o ../../src-tauri/binaries/ai-switch-tsnet-x86_64-pc-windows-msvc.exe .
```

The filename suffix must be your current Rust target triple — Tauri's `externalBin` mechanism looks the binary up by triple. `rustc -vV` prints your machine's `host:` triple.

## Release script tests

The release pipeline stages the assets, builds the release body, and generates and verifies `latest.json`. Those scripts ship with Node's built-in test runner:

```powershell
pnpm release:manifest:test
```

This runs the tests for four scripts under `scripts/`: `create-updater-manifest`, `verify-updater-signatures`, `stage-release-assets`, and `create-release-body`. CI executes this step in every platform build job, so run it locally after touching anything under `scripts/`.

## Run the desktop app

```powershell
pnpm tauri:dev
```

Per `beforeDevCommand` in `tauri.conf.json`, this starts Vite first (`http://127.0.0.1:1420`), then compiles and launches the Rust side. The first compile takes a while; incremental builds afterwards are much faster.

::: tip Development data is isolated from production data
Debug builds use `~/.ai-switch/ai-switch-dev.db`; release builds use `~/.ai-switch/ai-switch.db`. They share a data directory but keep separate database files, so `pnpm tauri:dev` can never touch the account data of your installed release.
:::

To work on the frontend only, run `pnpm dev` (Vite dev server bound to `127.0.0.1`). The frontend then uses the web transport, so you need a running web service or standalone server to provide `/api`.

## Build

### Desktop installers

```powershell
pnpm build
pnpm tauri:build
```

- `pnpm build` is `tsc && vite build`, producing `dist/`.
- `pnpm tauri:build` produces installers under `src-tauri/target/release/bundle/`.

Before packaging, confirm `src-tauri/binaries/` contains a sidecar binary for your target triple — otherwise Tauri fails because it cannot find the `externalBin`.

### Standalone server

```powershell
pnpm server:build          # debug, output at src-tauri/target/debug/ai-switch-server
pnpm server:build:release   # release, output at src-tauri/target/release/ai-switch-server
```

It is configured entirely through environment variables (PowerShell example):

```powershell
$env:AI_SWITCH_HOST = "127.0.0.1"
$env:AI_SWITCH_PORT = "3090"
$env:AI_SWITCH_TOKEN = "replace-me"
$env:AI_SWITCH_STATIC_DIR = "$PWD\dist"
.\src-tauri\target\debug\ai-switch-server.exe
```

The full variable list and deployment guidance are in [standalone server](/en/deploy/standalone-server).

## Run every check at once

Before opening a PR or cutting a release, run the whole set in CI order:

```powershell
pnpm typecheck
pnpm test:run
pnpm release:manifest:test
pnpm rust:check
pnpm rust:test
cd sidecar/ai-switch-tsnet; go test ./...
```

This is exactly the check set `.github/workflows/release.yml` runs inside every platform build job. Do not tag a release while any of them fails — see [release process](/en/dev/release).

## Working on this documentation site

The docs site is a separate pnpm project under `docs-site/`, with its own `package.json` and lockfile. It is not part of the root workspace.

```powershell
cd docs-site
pnpm install
pnpm docs:dev
```

`pnpm docs:dev` starts the VitePress dev server with hot reload, so markdown edits show up immediately.

To build and preview:

```powershell
pnpm docs:build
pnpm docs:preview
```

::: warning Use preview — do not open the HTML in dist directly
The site sets `base: "/ai-switch/"` because it is deployed under a GitHub Pages subpath. Opening `docs/.vitepress/dist/index.html` straight from the filesystem 404s every asset, since all of them are prefixed with `/ai-switch/`, and the page renders with no styling at all. `pnpm docs:preview` serves the build with the correct base (port 4173 by default), which is the only meaningful way to verify build output.
:::

### Conventions when writing docs

- **The site sets `cleanUrls: false`** (GitHub Pages cannot map `/foo` to `foo.html`), but internal links **still must not include a `.html` suffix** — VitePress appends it at build time. Chinese pages use `/guide/accounts`; English pages use `/en/guide/accounts`.
- **Linking to a page that does not exist fails the build.** VitePress dead-link checking is a hard failure: `pnpm docs:build` errors out and lists the broken links.
- **Never leave a bare double-brace interpolation in prose.** VitePress compiles markdown through Vue, so a pair of opening braces is parsed as an interpolation expression and the build fails. When you need to show something like a GitHub Actions expression, always put it inside a code block:

  ```yaml
  prerelease: ${{ contains(github.ref_name, '-rc') }}
  ```

  Fenced code blocks are safe; prose and inline code are not.
- Chinese and English pages map one to one. Sidebars are configured per locale in `docs/.vitepress/config.mts`, and the English sidebar keys must carry the `/en/` prefix.
- Every page needs `title` and `description` frontmatter — `transformPageData` uses them to generate canonical links and Open Graph tags.

### Docs CI

`.github/workflows/docs.yml` triggers on changes under `docs-site/**`. On pull requests it builds only, acting as a dead-link and syntax check; on pushes to `main` it builds and deploys to GitHub Pages. It checks out with `fetch-depth: 0` because `lastUpdated` reads each page's git commit time.

## See also

- [Architecture](/en/dev/architecture) — what each directory and module is responsible for
- [Release process](/en/dev/release) — tags, signing, and three-platform artifacts
- [Desktop deployment](/en/deploy/desktop)
- [FAQ](/en/faq)
