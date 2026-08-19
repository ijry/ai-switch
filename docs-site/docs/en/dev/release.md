---
title: Release Process
description: How AI Switch releases work — a version tag triggers GitHub Actions, which validates version consistency, builds for three platforms, signs updater artifacts, generates latest.json, and publishes to GitHub Releases.
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
5. **Compute platform variables.** Reads the host triple from `rustc -vV` and derives the updater platform id (`windows-x86_64`, `darwin-aarch64`, `linux-x86_64`, …) plus the sidecar and server binary paths.
6. **Frontend checks.** `pnpm typecheck`, `pnpm test:run`, `pnpm release:manifest:test`.
7. **Build the frontend.** `pnpm build`.
8. **Sidecar tests and build.** `go test ./...`, then `go build -trimpath -ldflags="-s -w"` into `src-tauri/binaries/ai-switch-tsnet-<triple><suffix>`, asserting the file actually exists afterwards.
9. **Rust checks.** `pnpm rust:check`, `pnpm rust:test`.
10. **Package with Tauri.** `pnpm tauri build --ci --bundles <that platform's formats>`.
11. **Build the standalone server.** `pnpm server:build:release`.
12. **Stage assets.** Recursively collects `.exe`/`.msi`/`.dmg`/`.deb`/`.AppImage`/`.zip`/`.tar.gz`/`.sig` from `src-tauri/target/release/bundle` and renames each to `ai-switch_<tag>_<platform>_<original>`. The server and sidecar binaries are zipped separately as `ai-switch-server_<tag>_<platform>.zip` and `ai-switch-tsnet_<tag>_<platform>.zip`. This step also asserts **at least one `.sig` file** was staged, failing otherwise.
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
2. **Read the release notes.** A paginated `gh api` query finds the release matching the tag and writes its body to `release-notes.md`. If nothing is found, the job exits with an error.
3. **Generate the updater manifest.** `scripts/create-updater-manifest.mjs` identifies the target platform from the subdirectory name and picks the updater asset by a preference order (Windows prefers `.exe` then `.msi`; macOS prefers `.tar.gz` then `.dmg`; Linux prefers `.AppImage` then `.deb`), writing `release-assets/latest.json`:

   ```bash
   node scripts/create-updater-manifest.mjs \
     --assets-dir release-assets \
     --tag "<tag>" \
     --repo "<owner/repo>" \
     --output release-assets/latest.json \
     --notes-file release-notes.md
   ```

4. **Verify signing key consistency.** `scripts/verify-updater-signatures.mjs` extracts the signer key ID from each `.sig`'s minisign payload and compares it against the key ID derived from `pubkey` in `tauri.conf.json`:

   ```bash
   node scripts/verify-updater-signatures.mjs \
     --manifest release-assets/latest.json \
     --tauri-config src-tauri/tauri.conf.json
   ```

   The point of this check: if the signing key is rotated without updating the public key in the config, already-installed clients fail verification and the update path breaks silently. Catching that before publishing is far cheaper than recovering afterwards.

5. **Promote to a real release.** `ncipollo/release-action` runs again with `draft: false`, `replacesArtifacts: true`, and `artifactErrorsFailBuild: true`, uploading everything under `release-assets/**/*.*` including `latest.json`.

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

A successful run attaches the following to the GitHub Release:

- **Windows:** an NSIS installer (`.exe`) and its `.sig`
- **macOS:** an `.app` archive and a `.dmg`, with `.sig` files
- **Linux:** a `.deb` and an `.AppImage`, with `.sig` files
- **Per platform:** `ai-switch-server_<tag>_<platform>.zip` (standalone server)
- **Per platform:** `ai-switch-tsnet_<tag>_<platform>.zip` (Tailscale sidecar)
- **`latest.json`:** the Tauri updater manifest that drives desktop auto-updates

How users get these is covered in [installation](/en/guide/installation); running the server build is covered in [standalone server](/en/deploy/standalone-server).

## See also

- [Local setup](/en/dev/local-setup) — the checks to pass before tagging
- [Architecture](/en/dev/architecture) — what each packaged piece actually is
- [Installation](/en/guide/installation)
- [FAQ](/en/faq)
