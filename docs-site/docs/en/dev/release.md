---
title: Release Process
description: How AI Switch releases work — a version tag triggers GitHub Actions, which validates version consistency, builds for three platforms, signs updater artifacts, generates latest.json, publishes to GitHub Releases, and then hands the release to Homebrew and WinGet from a separate workflow.
---

# Release Process

Releasing AI Switch is fully automated. Push a conforming version tag and `.github/workflows/release.yml` handles validation, three-platform builds, signing, and publication.

## What triggers it

The workflow listens for tag pushes only, and the tag must match `v*.*.*`:

```yaml
on:
  push:
    tags:
      - "v*.*.*"
```

The actual format check inside the job is stricter than that glob. The regex requires `v<major>.<minor>.<patch>` with an optional `-rc` / `-beta` / `-alpha` prerelease suffix, which may itself carry a `.<number>`. Valid examples:

```text
v0.6.7
v0.6.7-rc.1
v0.7.0-beta
v1.0.0-alpha.2
```

The concurrency group keys on `github.ref` with `cancel-in-progress: false`, so a release in flight is never interrupted by a later push.

## Three jobs

| Job | Runner | Responsibility |
| --- | --- | --- |
| `prepare` | `ubuntu-latest` | Validate the tag and version consistency, verify the tag's branch ancestry, create a draft release with generated notes |
| `build` | matrix: `windows-latest` / `macos-latest` / `ubuntu-latest` | Run all checks, build the sidecar, package the Tauri installers and standalone server, upload artifacts |
| `publish` | `ubuntu-latest` | Collect artifacts, generate and verify `latest.json`, promote the draft to a real release |

The dependency chain is `prepare` → `build` (parallel matrix) → `publish`. `build` sets `fail-fast: false`, so one platform failing does not cancel the others — you get to see every problem in a single run.

## prepare: two gates before anything is built

### 1. The version must match in three places

With the `v` prefix stripped, the tag must be **exactly identical, prerelease suffix included**, to:

- `version` in `package.json`
- `version` in `src-tauri/tauri.conf.json`

The check happens in two steps: first confirm `package.json` and `tauri.conf.json` agree with each other, then confirm the tag agrees with them. Either mismatch is an immediate `exit 1`.

In other words, tag `v0.6.7` requires both files to say `0.6.7`; tag `v0.7.0-rc.1` requires both files to say `0.7.0-rc.1`.

::: warning The usual mistake
Bumping `package.json` and forgetting `tauri.conf.json` (or the reverse) is by far the most common way to trip this gate. Confirm both before tagging. The project's own version in `src-tauri/Cargo.toml` and `Cargo.lock` should be kept in sync too — CI does not check it, but a mismatch leaves the build artifacts with inconsistent metadata.
:::

### 2. The tag must be based on the default branch

```bash
git fetch origin "$DEFAULT_BRANCH"
tag_commit="$(git rev-list -n 1 "$GITHUB_REF_NAME")"
if ! git merge-base --is-ancestor "$tag_commit" "origin/$DEFAULT_BRANCH"; then
  echo "Tag ${GITHUB_REF_NAME} is not based on ${DEFAULT_BRANCH}."
  exit 1
fi
```

The tagged commit must be an ancestor of the default branch. Tagging on a feature branch and pushing it is rejected — this check exists to stop unmerged code from shipping by accident.

### 3. Create the draft release

Once both gates pass, `ncipollo/release-action` creates a **draft** release (`draft: true`) with automatically generated notes (`generateReleaseNotes: true`). Whether it is marked as a prerelease is derived from the tag name:

```yaml
prerelease: ${{ contains(github.ref_name, '-rc') || contains(github.ref_name, '-beta') || contains(github.ref_name, '-alpha') }}
```

The draft state matters: users never see a half-populated release while builds run, and it only flips to published after `publish` succeeds.

## build: the three-platform matrix

| Label | Runner | Bundle formats |
| --- | --- | --- |
| Windows | `windows-latest` | `nsis` |
| macOS | `macos-latest` | `app`, `dmg` |
| Linux | `ubuntu-latest` | `deb`, `appimage` |

Each platform runs the same sequence:

1. **Validate the signing secret.** If `TAURI_SIGNING_PRIVATE_KEY` is empty the job throws immediately — unsigned updater artifacts are never produced.
2. **Linux system dependencies.** Linux only: `apt-get install` for `libwebkit2gtk-4.1-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`, `patchelf`, `libgtk-3-dev`.
3. **Toolchains.** pnpm 10.12.4, Node 22 (with pnpm cache), stable Rust, stable Go (cached on `sidecar/ai-switch-tsnet/go.sum`).
4. **Install dependencies.** `pnpm install --frozen-lockfile`.
5. **Compute platform variables.** Reads the host triple from `rustc -vV` and derives the updater platform id (`windows-x86_64`, `darwin-aarch64`, `linux-x86_64`, …), `APP_VERSION`, plus the sidecar and server binary paths.
6. **Frontend checks.** `pnpm typecheck`, `pnpm test:run`, `pnpm release:manifest:test`.
7. **Build the frontend.** `pnpm build`.
8. **Sidecar tests and build.** `go test ./...`, then `go build -trimpath -ldflags="-s -w"` into `src-tauri/binaries/ai-switch-tsnet-<triple><suffix>`, asserting the file actually exists afterwards.
9. **Rust checks.** `pnpm rust:check`, `pnpm rust:test`.
10. **Package with Tauri.** `pnpm tauri build --ci --bundles <that platform's formats>`.
11. **Build the standalone server.** `pnpm server:build:release`.
12. **Stage assets.** `scripts/stage-release-assets.mjs` renames the installers from `src-tauri/target/release/bundle` to `ai-switch-<version>-<platform>` (`.exe` always gets the `-setup` suffix), names the updater-only `.app.tar.gz` / `.nsis.zip` payloads `ai-switch-updater-<version>-<platform>`, and keeps every `.sig` beside the file it signs. The deb intermediates (`control.tar.gz`, `data.tar.gz`, `debian-binary`) and anything inside the built `.app` are skipped. Missing installers or missing signatures fail the step. The server and sidecar binaries are zipped separately as `ai-switch-server_<tag>_<platform>.zip` and `ai-switch-tsnet_<tag>_<platform>.zip`.

    The naming is not arbitrary: the GitHub release page sorts assets by name and folds all but the first few behind "Show all N assets". Putting the version right after `ai-switch-` sorts the installers ahead of `ai-switch-server_` and `ai-switch-tsnet_`, so the Windows `.exe` and the macOS `.dmg` people came for stay visible. `<platform>` stays the updater platform id rather than a friendlier `windows-x64` because the package-manager publishing picks its installers by that token (see below).
13. **Upload artifacts.** Named by updater platform id, with `if-no-files-found: error`.

Note that CI reruns the full check suite on each platform. A single release therefore executes the tests three times, which is how platform-specific problems surface here rather than in the wild.

## Updater signing

Tauri's updater requires minisign signatures alongside the installers. The key is injected through repository secrets:

| Secret | Required | Notes |
| --- | --- | --- |
| `TAURI_SIGNING_PRIVATE_KEY` | **Yes** | The minisign private key. Missing it fails the first step of `build` |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Optional | The key's passphrase, only needed if the key has one |

The matching public key is committed in `plugins.updater.pubkey` in `src-tauri/tauri.conf.json`, and the update endpoint points at:

```text
https://github.com/ijry/ai-switch/releases/latest/download/latest.json
```

`bundle.createUpdaterArtifacts: true` is what makes Tauri emit the updater artifacts and their `.sig` files during packaging.

## publish: manifest generation and signature verification

1. **Download every artifact.** `actions/download-artifact` pulls all three platforms into `release-assets/`, one subdirectory per platform.
2. **Restore the release notes.** The `prepare` job's `release_notes` output is written back to `release-notes.md`. An empty file fails the job.
3. **Generate the updater manifest.** `scripts/create-updater-manifest.mjs` identifies the target platform from the subdirectory name and picks the updater asset by a preference order (Windows prefers `.exe` then `.msi`; macOS prefers `.tar.gz` then `.dmg`; Linux prefers `.AppImage` then `.deb`), writing `release-assets/latest.json`:

   ```bash
   node scripts/create-updater-manifest.mjs \
     --assets-dir release-assets \
     --tag "<tag>" \
     --repo "<owner/repo>" \
     --output release-assets/latest.json \
     --notes-file release-notes.md
   ```

4. **Verify signing key consistency.** `scripts/verify-updater-signatures.mjs` extracts the signer key ID from each platform's signature payload in the manifest and compares it against the key ID derived from `pubkey` in `tauri.conf.json`:

   ```bash
   node scripts/verify-updater-signatures.mjs \
     --manifest release-assets/latest.json \
     --tauri-config src-tauri/tauri.conf.json
   ```

   The point of this check: if the signing key is rotated without updating the public key in the config, already-installed clients fail verification and the update path breaks silently. Catching that before publishing is far cheaper than recovering afterwards.

5. **Delete the `.sig` files.** The signatures are inlined in `latest.json` by now, and the client only ever reads the manifest — it never fetches a sibling `.sig`. So `find release-assets -name '*.sig' -delete` keeps them from spending the few asset rows the release page shows.
6. **Build the release body.** `scripts/create-release-body.mjs` scans the platform subdirectories for installers and prepends a bilingual download table (one row each for Windows, macOS, and Linux, plus a line for the standalone server) to the tag message, writing `release-body.md`:

   ```bash
   node scripts/create-release-body.mjs \
     --assets-dir release-assets \
     --tag "<tag>" \
     --repo "<owner/repo>" \
     --output release-body.md \
     --notes-file release-notes.md
   ```

   The table goes into the GitHub release body only. The manifest's `notes` stay the verbatim tag message, because the desktop client splits it on the 29-hyphen separator to pick a language and a stray table would leak into the changelog.

7. **Promote to a real release.** `ncipollo/release-action` runs again with `draft: false`, `bodyFile: release-body.md`, `replacesArtifacts: true`, and `artifactErrorsFailBuild: true`, uploading everything under `release-assets/**/*.*` including `latest.json`.

## Cutting a release

### Stable

Confirm the working tree is clean, versions are in sync, and CI is green on `main`, then:

```bash
git tag -a v0.6.8 -m "Release v0.6.8"
git push origin main
git push origin v0.6.8
```

### Prerelease

A `-rc` / `-beta` / `-alpha` suffix in the tag name marks it as a prerelease automatically:

```bash
git tag -a v0.7.0-rc.1 -m "Release v0.7.0-rc.1"
git push origin v0.7.0-rc.1
```

Remember that `package.json` and `src-tauri/tauri.conf.json` must both read the full `0.7.0-rc.1`.

### Recovering from a bad tag

If the tag has not been pushed yet, just delete and redo it:

```bash
git tag -d v0.6.8
```

If it was pushed and the workflow failed, delete the remote tag first. Note that GitHub may still hold a draft release, which you should remove manually — otherwise `allowUpdates: true` makes the next run reuse it:

```bash
git push origin :refs/tags/v0.6.8
git tag -d v0.6.8
```

The better habit is **running the full check suite locally before tagging** — see "Run every check at once" in [local setup](/en/dev/local-setup). A three-platform release run is not fast, and letting CI discover problems you could have caught locally wastes real time.

## What a release produces

A successful run attaches the following to the GitHub Release, in the order the asset list shows them:

- **Windows:** `ai-switch-<version>-windows-x86_64-setup.exe`
- **macOS:** `ai-switch-<version>-darwin-aarch64.dmg`
- **Linux:** `ai-switch-<version>-linux-x86_64.AppImage` and `ai-switch-<version>-linux-x86_64.deb`
- **Per platform:** `ai-switch-server_<tag>_<platform>.zip` (standalone server)
- **Per platform:** `ai-switch-tsnet_<tag>_<platform>.zip` (Tailscale sidecar)
- **macOS:** `ai-switch-updater-<version>-darwin-aarch64.app.tar.gz` (only the auto-updater downloads it)
- **`latest.json`:** the Tauri updater manifest that drives desktop auto-updates

The `.sig` files are not published as separate assets; their signatures live inside `latest.json`. The release body also opens with a download table pointing straight at the first three groups above.

How users get these is covered in [installation](/en/guide/installation); running the server build is covered in [standalone server](/en/deploy/standalone-server).

## Publishing to package managers (Homebrew / WinGet)

`.github/workflows/package-managers.yml` hands a release that is **already published** to the package managers. Keeping it out of `release.yml` is deliberate: a winget submission waits on Microsoft's review and a Homebrew push can fail on an expired token, and neither should be able to hold up or fail the release itself. The other direction matters too — re-submitting a tag from months ago needs no rebuild.

### What triggers it

| Trigger | Behaviour |
| --- | --- |
| A release is published | Runs automatically, tag taken from the event |
| Manual `workflow_dispatch` | Pass `tag` to publish any past release; leave it empty for the latest one |

A manual run has three more switches: `homebrew` and `winget` can each be turned off, and `dry_run` renders and checks the manifests without touching any external repository.

Drafts and prereleases (`-rc` / `-beta` / `-alpha`) are always skipped, with the reason in the log rather than a failure: a draft's assets have no public download URL yet, and a prerelease in the tap would reach everyone who runs `brew upgrade`.

### What has to be configured

Both paths write to **someone else's repository**, so both need a repository secret. **A missing secret does not fail the workflow** — it logs a warning, skips that path, and lets the other one run.

| Secret / Variable | Kind | Purpose |
| --- | --- | --- |
| `HOMEBREW_TAP_TOKEN` | secret | PAT with `contents: write` on the tap repository |
| `HOMEBREW_TAP_REPO` | variable, optional | Tap repository, defaults to `ijry/homebrew-ai-switch` |
| `WINGET_TOKEN` | secret | **Classic** PAT, `public_repo` scope only |
| `WINGET_FORK_USER` | variable, optional | Account holding the winget-pkgs fork, defaults to the repo owner |

::: warning The WinGet token has to be a classic PAT
The tool that opens the pull request is Komac, which goes through GitHub's GraphQL API — and a fine-grained token can only reach GraphQL for resources whose owner matches the token's own. The target is `microsoft/winget-pkgs`, so fine-grained tokens and GitHub Apps both fail here.
:::

### Homebrew: one-time setup

1. Create a **public** repository `ijry/homebrew-ai-switch`. The name has to start with `homebrew-` for `brew tap ijry/ai-switch` to resolve.
2. Generate a PAT with `contents: write` on it and store it as `HOMEBREW_TAP_TOKEN`.

From then on each release renders the cask on `macos-latest`, stages it in a local tap, **actually runs `brew install --cask`**, asserts `/Applications/AI Switch.app` exists with the quarantine flag cleared, and only then pushes to the tap. For an ad-hoc signed bundle that install check is the one that matters: a cask that parses fine can still install an app that refuses to open.

::: tip The cask does the Gatekeeper dance for the user
The macOS bundle is ad-hoc signed and never notarized (see the section in [installation](/en/guide/installation)), so the cask carries a `postflight` that runs `xattr -dr com.apple.quarantine` on the copy Homebrew just placed. It touches that one path and changes no system setting, but it saves the user the manual "System Settings → Open Anyway" trip.

The cask also declares `depends_on arch: :arm64` (CI only builds Apple Silicon) and `auto_updates true` (the app's own updater replaces the bundle, so what Homebrew recorded goes stale by itself).
:::

### WinGet: the first version has to be submitted by hand

The automation can only do **version bumps**, never the initial listing. `winget-releaser`'s very first step checks whether the package exists in `microsoft/winget-pkgs` and errors out if it does not:

```text
::error::Package ijry.AISwitch does not exist in the winget-pkgs repository.
Please add atleast one version of the package before using this action.
```

So the one-time setup is three steps:

1. Fork `microsoft/winget-pkgs` under `ijry` — the tooling will not create the fork for you.
2. Submit the first version of `ijry.AISwitch` by hand with [Komac](https://github.com/russellbanks/Komac) or [wingetcreate](https://github.com/microsoft/winget-create) and wait for a winget maintainer to merge it. The identifier is case-sensitive and has to match its directory path exactly (`manifests/i/ijry/AISwitch/<version>/`).
3. Generate a classic PAT (`public_repo`) and store it as `WINGET_TOKEN`.

Each release then opens a pull request against `microsoft/winget-pkgs`. **A Microsoft maintainer has to merge it before the version reaches users**, and that step is outside our control.

::: tip The installer itself does not need code signing
Nothing in winget-pkgs' policy docs requires code signing, and `SignatureSha256` only applies to MSIX/APPX. Its validation pipeline cares about other things: multi-engine antivirus scanning, a silent install as a non-elevated user, and post-install registry entries that agree with the manifest's `Publisher` and `PackageName`.

Tauri's NSIS installer defaults to `currentUser` mode, so it needs no elevation, and `InstallerType: nullsoft` needs no hand-written silent switches — the winget client supplies `/S` once it recognises nullsoft.
:::

### What users run

Once both setups above are done and each channel has its first version landed:

```bash
# macOS (Apple Silicon)
brew tap ijry/ai-switch
brew install --cask ai-switch
```

```powershell
# Windows
winget install ijry.AISwitch
```

Until then neither command finds the package, which is why [installation](/en/guide/installation) still only documents downloading from Releases.

### How the manifests are generated

`scripts/create-package-manifests.mjs` starts from the release's API response and does three things:

1. **Picks the installers.** It matches on the updater platform token (`darwin-aarch64`, `windows-x86_64`) plus the extension, excluding `.sig`, `.app.tar.gz` and `.nsis.zip` — payloads only the updater ever downloads. The repo has shipped two asset naming schemes and both match; zero matches or more than one is an error rather than a guess.
2. **Resolves sha256.** The releases API now reports a `digest` per asset, so the usual path never pulls the 31 MB dmg down to hash it. Assets from older releases have no digest and fall back to a streaming download.
3. **Renders the cask and writes winget's inputs to `summary.json`.** The `installers-regex` handed to `winget-releaser` is an anchored pattern built from the file name step 1 already resolved, so its own second lookup cannot disagree with step 1.

`pnpm release:manifest:test` covers the script — every platform in `release.yml` runs it — and the cases pin v0.8.0's real asset list.

## See also

- [Local setup](/en/dev/local-setup) — the checks to pass before tagging
- [Architecture](/en/dev/architecture) — what each packaged piece actually is
- [Installation](/en/guide/installation)
- [FAQ](/en/faq)
