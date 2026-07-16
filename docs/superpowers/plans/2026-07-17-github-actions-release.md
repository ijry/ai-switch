# GitHub Actions Release Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a manual GitHub Actions workflow that builds and publishes signed Windows, macOS, and Linux release binaries for AI Switch.

**Architecture:** Keep release orchestration in one workflow and keep updater manifest generation in a small Node script that can be tested locally. Matrix build jobs produce platform-specific assets as GitHub Actions artifacts; a single publish job creates or updates the GitHub Release only after every build job succeeds.

**Tech Stack:** GitHub Actions, pnpm 10.12.4, Node 22, Rust stable, Go stable, Tauri 2, Node `node:test`.

## Global Constraints

- Work directly on `main`; do not create or switch branches or worktrees.
- The release workflow uses `workflow_dispatch` only.
- Workflow inputs are `tag`, `release_name`, `draft`, and `prerelease`.
- Platform jobs are Windows, macOS, and Linux.
- Desktop bundle targets are Windows `nsis`, macOS `dmg`, and Linux `deb` plus `appimage`.
- Updater signing requires `TAURI_SIGNING_PRIVATE_KEY`; `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` is optional.
- If updater signing is not configured, release builds fail before publishing assets.
- The updater endpoint remains `https://github.com/ijry/ai-switch/releases/latest/download/latest.json`.
- Existing local development commands keep working.
- Do not overwrite the unrelated existing working tree modification in `src-tauri/Cargo.toml`.

---

## File Structure

- Create `.github/workflows/release.yml`: manual release workflow, platform matrix, artifact publication.
- Create `scripts/create-updater-manifest.mjs`: generate Tauri updater `latest.json` from signed release assets.
- Create `scripts/create-updater-manifest.test.mjs`: test updater manifest generation with temporary fake assets.
- Modify `package.json`: add `release:manifest:test` script only.
- Modify `src-tauri/tauri.conf.json`: change bundle targets from Windows-only `nsis` to cross-platform `all`; workflow still passes explicit `--bundles` per OS.
- Modify `src-tauri/binaries/README.md`: document target-triple sidecar filenames for CI.
- Modify `README.md`: add a concise release automation section and required GitHub secrets.

---

### Task 1: Add Tested Updater Manifest Generator

**Files:**
- Create: `scripts/create-updater-manifest.mjs`
- Create: `scripts/create-updater-manifest.test.mjs`
- Modify: `package.json`

**Interfaces:**
- Consumes: release asset directories named by updater platform, for example `release-assets/windows-x86_64`.
- Produces: `latest.json` with `version`, `notes`, `pub_date`, and `platforms`.
- CLI: `node scripts/create-updater-manifest.mjs --assets-dir release-assets --tag v0.1.0 --repo ijry/ai-switch --output release-assets/latest.json`

- [ ] **Step 1: Add the failing test file**

Create `scripts/create-updater-manifest.test.mjs`:

```js
import assert from "node:assert/strict";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { test } from "node:test";
import { pathToFileURL } from "node:url";

const moduleUrl = pathToFileURL(path.resolve("scripts/create-updater-manifest.mjs")).href;
const { createManifest } = await import(moduleUrl);

test("creates updater manifest from signed platform assets", async () => {
    const root = await mkdtemp(path.join(tmpdir(), "ai-switch-release-"));

  try {
    const winDir = path.join(root, "windows-x86_64");
    const linuxDir = path.join(root, "linux-x86_64");
    await mkdir(winDir, { recursive: true });
    await mkdir(linuxDir, { recursive: true });

    await writeFile(path.join(winDir, "ai-switch_v0.1.0_windows-x86_64_setup.exe"), "binary");
    await writeFile(path.join(winDir, "ai-switch_v0.1.0_windows-x86_64_setup.exe.sig"), "win-signature\n");
    await writeFile(path.join(linuxDir, "ai-switch_v0.1.0_linux-x86_64.AppImage"), "binary");
    await writeFile(path.join(linuxDir, "ai-switch_v0.1.0_linux-x86_64.AppImage.sig"), "linux-signature\n");

    const output = path.join(root, "latest.json");
    await createManifest({
      assetsDir: root,
      tag: "v0.1.0",
      repo: "ijry/ai-switch",
      output,
      pubDate: "2026-07-17T00:00:00.000Z",
    });

    const manifest = JSON.parse(await readFile(output, "utf8"));
    assert.equal(manifest.version, "0.1.0");
    assert.equal(manifest.pub_date, "2026-07-17T00:00:00.000Z");
    assert.equal(manifest.platforms["windows-x86_64"].signature, "win-signature");
    assert.equal(manifest.platforms["linux-x86_64"].signature, "linux-signature");
    assert.equal(
      manifest.platforms["windows-x86_64"].url,
      "https://github.com/ijry/ai-switch/releases/download/v0.1.0/ai-switch_v0.1.0_windows-x86_64_setup.exe",
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("fails when a platform directory has no signed updater asset", async () => {
  const root = await mkdtemp(path.join(tmpdir(), "ai-switch-release-"));

  try {
    const winDir = path.join(root, "windows-x86_64");
    await mkdir(winDir, { recursive: true });
    await writeFile(path.join(winDir, "ai-switch_v0.1.0_windows-x86_64_setup.exe"), "binary");

    await assert.rejects(
      () =>
        createManifest({
          assetsDir: root,
          tag: "v0.1.0",
          repo: "ijry/ai-switch",
          output: path.join(root, "latest.json"),
          pubDate: "2026-07-17T00:00:00.000Z",
        }),
      /No signed updater asset found/,
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `node --test scripts/create-updater-manifest.test.mjs`

Expected: FAIL with an import error for `scripts/create-updater-manifest.mjs`.

- [ ] **Step 3: Add the manifest generator**

Create `scripts/create-updater-manifest.mjs`:

```js
import { mkdir, readdir, readFile, stat, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const updaterAssetPreference = new Map([
  ["windows-x86_64", [/\.exe$/i, /\.msi$/i]],
  ["windows-aarch64", [/\.exe$/i, /\.msi$/i]],
  ["darwin-x86_64", [/\.dmg$/i]],
  ["darwin-aarch64", [/\.dmg$/i]],
  ["linux-x86_64", [/\.AppImage$/i, /\.deb$/i]],
  ["linux-aarch64", [/\.AppImage$/i, /\.deb$/i]],
]);

function parseArgs(argv) {
  const args = new Map();

  for (let index = 0; index < argv.length; index += 2) {
    const key = argv[index];
    const value = argv[index + 1];
    if (!key?.startsWith("--") || !value) {
      throw new Error(`Invalid argument sequence near ${key ?? "<end>"}`);
    }
    args.set(key.slice(2), value);
  }

  return {
    assetsDir: required(args, "assets-dir"),
    tag: required(args, "tag"),
    repo: required(args, "repo"),
    output: required(args, "output"),
    pubDate: args.get("pub-date") ?? new Date().toISOString(),
  };
}

function required(args, key) {
  const value = args.get(key);
  if (!value) {
    throw new Error(`Missing --${key}`);
  }
  return value;
}

async function listFilesRecursive(root) {
  const entries = await readdir(root, { withFileTypes: true });
  const files = [];

  for (const entry of entries) {
    const entryPath = path.join(root, entry.name);
    if (entry.isDirectory()) {
      files.push(...(await listFilesRecursive(entryPath)));
    } else if (entry.isFile()) {
      files.push(entryPath);
    }
  }

  return files;
}

function platformFromDirectory(assetsDir, filePath) {
  const relative = path.relative(assetsDir, filePath);
  const [platform] = relative.split(path.sep);
  return platform;
}

function releaseUrl(repo, tag, assetName) {
  return `https://github.com/${repo}/releases/download/${encodeURIComponent(tag)}/${encodeURIComponent(assetName)}`;
}

function versionFromTag(tag) {
  return tag.replace(/^v/i, "");
}

function pickSignedAsset(platform, signedAssets) {
  const preferences = updaterAssetPreference.get(platform) ?? [];

  for (const pattern of preferences) {
    const match = signedAssets.find((asset) => pattern.test(asset.assetPath));
    if (match) {
      return match;
    }
  }

  return signedAssets[0];
}

export async function createManifest({ assetsDir, tag, repo, output, pubDate = new Date().toISOString() }) {
  const rootStat = await stat(assetsDir);
  if (!rootStat.isDirectory()) {
    throw new Error(`Assets path is not a directory: ${assetsDir}`);
  }

  const files = await listFilesRecursive(assetsDir);
  const signatures = files.filter((file) => file.endsWith(".sig"));
  const signedByPlatform = new Map();

  for (const signaturePath of signatures) {
    const assetPath = signaturePath.slice(0, -".sig".length);
    if (!files.includes(assetPath)) {
      throw new Error(`Signature has no matching asset: ${signaturePath}`);
    }

    const platform = platformFromDirectory(assetsDir, signaturePath);
    if (!platform) {
      throw new Error(`Signed asset must be inside a platform directory: ${signaturePath}`);
    }

    const entries = signedByPlatform.get(platform) ?? [];
    entries.push({ assetPath, signaturePath });
    signedByPlatform.set(platform, entries);
  }

  const platforms = {};
  const platformDirectories = (await readdir(assetsDir, { withFileTypes: true }))
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name)
    .sort();

  for (const platform of platformDirectories) {
    const signedAssets = signedByPlatform.get(platform) ?? [];
    const picked = pickSignedAsset(platform, signedAssets);
    if (!picked) {
      throw new Error(`No signed updater asset found for ${platform}`);
    }

    const signature = (await readFile(picked.signaturePath, "utf8")).trim();
    const assetName = path.basename(picked.assetPath);
    platforms[platform] = {
      signature,
      url: releaseUrl(repo, tag, assetName),
    };
  }

  const manifest = {
    version: versionFromTag(tag),
    notes: "",
    pub_date: pubDate,
    platforms,
  };

  await mkdir(path.dirname(output), { recursive: true });
  await writeFile(output, `${JSON.stringify(manifest, null, 2)}\n`);
  return manifest;
}

const isMain = process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1]);

if (isMain) {
  createManifest(parseArgs(process.argv.slice(2))).catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  });
}
```

- [ ] **Step 4: Add the package script**

Modify `package.json` scripts by adding this entry after `test:run`:

```json
"release:manifest:test": "node --test scripts/create-updater-manifest.test.mjs",
```

- [ ] **Step 5: Run the manifest tests**

Run: `pnpm release:manifest:test`

Expected: PASS with two `node:test` tests.

- [ ] **Step 6: Commit Task 1**

Run:

```powershell
git add package.json scripts/create-updater-manifest.mjs scripts/create-updater-manifest.test.mjs
git commit -m "feat: add updater manifest generator"
```

---

### Task 2: Make Tauri Bundle Configuration Cross-Platform

**Files:**
- Modify: `src-tauri/tauri.conf.json`
- Modify: `src-tauri/binaries/README.md`

**Interfaces:**
- Consumes: Tauri CLI `--bundles` from the workflow.
- Produces: cross-platform-capable base Tauri bundle configuration.

- [ ] **Step 1: Update Tauri bundle targets**

In `src-tauri/tauri.conf.json`, replace:

```json
"targets": [
  "nsis"
],
```

with:

```json
"targets": "all",
```

- [ ] **Step 2: Update sidecar binary documentation**

Replace `src-tauri/binaries/README.md` with:

```markdown
# Built sidecar binaries for Tauri externalBin.

Tauri resolves `binaries/ai-switch-tsnet` to a target-triple-specific file during bundling.

Examples:

```text
ai-switch-tsnet-x86_64-pc-windows-msvc.exe
ai-switch-tsnet-x86_64-apple-darwin
ai-switch-tsnet-aarch64-apple-darwin
ai-switch-tsnet-x86_64-unknown-linux-gnu
```

Generate a local Windows binary before packaging:

```powershell
cd sidecar/ai-switch-tsnet
go build -o ..\..\src-tauri\binaries\ai-switch-tsnet-x86_64-pc-windows-msvc.exe .
```

The release workflow detects the Rust host target triple and writes the sidecar binary to the matching filename before `tauri build` runs.
```

- [ ] **Step 3: Validate JSON**

Run:

```powershell
node -e "JSON.parse(require('fs').readFileSync('src-tauri/tauri.conf.json','utf8')); console.log('valid')"
```

Expected: `valid`

- [ ] **Step 4: Commit Task 2**

Run:

```powershell
git add src-tauri/tauri.conf.json src-tauri/binaries/README.md
git commit -m "chore: allow cross-platform tauri bundles"
```

---

### Task 3: Add Manual Release Workflow

**Files:**
- Create: `.github/workflows/release.yml`

**Interfaces:**
- Consumes: GitHub secrets `TAURI_SIGNING_PRIVATE_KEY` and optional `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
- Consumes: workflow inputs `tag`, `release_name`, `draft`, and `prerelease`.
- Produces: GitHub Release assets and `latest.json`.

- [ ] **Step 1: Create the workflow directory**

Run:

```powershell
New-Item -ItemType Directory -Force .github\workflows
```

Expected: `.github/workflows` exists.

- [ ] **Step 2: Add `.github/workflows/release.yml`**

Create `.github/workflows/release.yml`:

```yaml
name: Release

on:
  workflow_dispatch:
    inputs:
      tag:
        description: "Release tag, for example v0.1.0"
        required: true
        type: string
      release_name:
        description: "Release display name. Defaults to tag when empty."
        required: false
        type: string
      draft:
        description: "Create or keep the release as a draft"
        required: true
        default: true
        type: boolean
      prerelease:
        description: "Mark the release as a prerelease"
        required: true
        default: false
        type: boolean

permissions:
  contents: write

jobs:
  build:
    name: Build ${{ matrix.label }}
    runs-on: ${{ matrix.os }}
    strategy:
      fail-fast: false
      matrix:
        include:
          - label: Windows
            os: windows-latest
            bundles: nsis
          - label: macOS
            os: macos-latest
            bundles: dmg
          - label: Linux
            os: ubuntu-latest
            bundles: "deb appimage"

    env:
      CI: true
      TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
      TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}

    steps:
      - name: Checkout
        uses: actions/checkout@v4

      - name: Validate updater signing secret
        shell: pwsh
        run: |
          if (-not $env:TAURI_SIGNING_PRIVATE_KEY) {
            throw "TAURI_SIGNING_PRIVATE_KEY is required for signed updater release assets."
          }

      - name: Install Linux Tauri dependencies
        if: runner.os == 'Linux'
        shell: bash
        run: |
          sudo apt-get update
          sudo apt-get install -y \
            libwebkit2gtk-4.1-dev \
            libappindicator3-dev \
            librsvg2-dev \
            patchelf \
            libgtk-3-dev

      - name: Setup pnpm
        uses: pnpm/action-setup@v4
        with:
          version: 10.12.4

      - name: Setup Node
        uses: actions/setup-node@v4
        with:
          node-version: 22
          cache: pnpm

      - name: Setup Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Setup Go
        uses: actions/setup-go@v5
        with:
          go-version: stable
          cache-dependency-path: sidecar/ai-switch-tsnet/go.sum

      - name: Install frontend dependencies
        run: pnpm install --frozen-lockfile

      - name: Compute platform variables
        shell: pwsh
        run: |
          $hostTriple = ((rustc -vV | Select-String '^host:') -replace '^host:\s*', '').Trim()
          $exeSuffix = ""
          if ($env:RUNNER_OS -eq "Windows") {
            $exeSuffix = ".exe"
          }

          $arch = switch ($env:RUNNER_ARCH) {
            "X64" { "x86_64" }
            "ARM64" { "aarch64" }
            default { throw "Unsupported runner architecture: $env:RUNNER_ARCH" }
          }

          $platform = switch ($env:RUNNER_OS) {
            "Windows" { "windows-$arch" }
            "macOS" { "darwin-$arch" }
            "Linux" { "linux-$arch" }
            default { throw "Unsupported runner OS: $env:RUNNER_OS" }
          }

          "HOST_TRIPLE=$hostTriple" >> $env:GITHUB_ENV
          "EXE_SUFFIX=$exeSuffix" >> $env:GITHUB_ENV
          "UPDATER_PLATFORM=$platform" >> $env:GITHUB_ENV
          "SIDECAR_BIN=src-tauri/binaries/ai-switch-tsnet-$hostTriple$exeSuffix" >> $env:GITHUB_ENV
          "SERVER_BIN=src-tauri/target/release/ai-switch-server$exeSuffix" >> $env:GITHUB_ENV

      - name: Run frontend checks
        run: |
          pnpm typecheck
          pnpm test:run
          pnpm release:manifest:test

      - name: Run Rust checks
        run: |
          pnpm rust:check
          pnpm rust:test

      - name: Run sidecar tests
        working-directory: sidecar/ai-switch-tsnet
        run: go test ./...

      - name: Build sidecar binary
        shell: pwsh
        run: |
          New-Item -ItemType Directory -Force src-tauri/binaries | Out-Null
          Push-Location sidecar/ai-switch-tsnet
          go build -trimpath -ldflags="-s -w" -o "../../$env:SIDECAR_BIN" .
          Pop-Location
          if (-not (Test-Path $env:SIDECAR_BIN)) {
            throw "Missing sidecar binary: $env:SIDECAR_BIN"
          }

      - name: Build standalone server
        run: pnpm server:build:release

      - name: Build Tauri bundle
        shell: pwsh
        run: |
          $bundles = "${{ matrix.bundles }}".Split(" ", [System.StringSplitOptions]::RemoveEmptyEntries)
          pnpm tauri build --ci --bundles @bundles

      - name: Stage release assets
        shell: pwsh
        run: |
          $tag = "${{ inputs.tag }}"
          $dest = "release-assets/$env:UPDATER_PLATFORM"
          New-Item -ItemType Directory -Force $dest | Out-Null

          $bundleFiles = Get-ChildItem src-tauri/target/release/bundle -Recurse -File |
            Where-Object {
              $_.Name -match '\.(exe|msi|dmg|deb|AppImage|sig)$'
            }

          if (-not $bundleFiles) {
            throw "No Tauri bundle assets found."
          }

          foreach ($file in $bundleFiles) {
            $safeName = "ai-switch_${tag}_$env:UPDATER_PLATFORM`_$($file.Name)" -replace '[^\w.\-]+', '-'
            Copy-Item $file.FullName (Join-Path $dest $safeName)
          }

          $serverArchive = Join-Path $dest "ai-switch-server_${tag}_$env:UPDATER_PLATFORM.zip"
          $sidecarArchive = Join-Path $dest "ai-switch-tsnet_${tag}_$env:UPDATER_PLATFORM.zip"
          Compress-Archive -Path $env:SERVER_BIN -DestinationPath $serverArchive -Force
          Compress-Archive -Path $env:SIDECAR_BIN -DestinationPath $sidecarArchive -Force

          $signatureCount = (Get-ChildItem $dest -Filter *.sig -File).Count
          if ($signatureCount -lt 1) {
            throw "No Tauri updater signature files were staged."
          }

      - name: Upload release assets
        uses: actions/upload-artifact@v4
        with:
          name: release-assets-${{ env.UPDATER_PLATFORM }}
          path: release-assets
          if-no-files-found: error

  publish:
    name: Publish GitHub Release
    runs-on: ubuntu-latest
    needs: build

    steps:
      - name: Checkout
        uses: actions/checkout@v4

      - name: Setup Node
        uses: actions/setup-node@v4
        with:
          node-version: 22

      - name: Download release assets
        uses: actions/download-artifact@v4
        with:
          pattern: release-assets-*
          path: release-assets
          merge-multiple: true

      - name: Generate updater manifest
        run: |
          node scripts/create-updater-manifest.mjs \
            --assets-dir release-assets \
            --tag "${{ inputs.tag }}" \
            --repo "${{ github.repository }}" \
            --output release-assets/latest.json

      - name: Create or update release
        uses: ncipollo/release-action@v1
        with:
          tag: ${{ inputs.tag }}
          commit: ${{ github.sha }}
          name: ${{ inputs.release_name != '' && inputs.release_name || inputs.tag }}
          draft: ${{ inputs.draft }}
          prerelease: ${{ inputs.prerelease }}
          allowUpdates: true
          replacesArtifacts: true
          artifactErrorsFailBuild: true
          artifacts: "release-assets/**/*"
```

- [ ] **Step 3: Validate workflow YAML parses**

Run:

```powershell
npx --yes yaml-lint .github/workflows/release.yml
```

Expected: PASS with no YAML syntax errors.

- [ ] **Step 4: Commit Task 3**

Run:

```powershell
git add .github/workflows/release.yml
git commit -m "ci: add manual cross-platform release workflow"
```

---

### Task 4: Document Release Operation

**Files:**
- Modify: `README.md`

**Interfaces:**
- Consumes: release workflow from Task 3.
- Produces: maintainer-facing release instructions.

- [ ] **Step 1: Add a release automation section**

Add this section after the existing desktop build commands in `README.md`:

```markdown
## Release Automation

GitHub Actions can build cross-platform release assets manually from the **Release** workflow.

Required repository secret:

- `TAURI_SIGNING_PRIVATE_KEY`

Optional repository secret:

- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`

Run the workflow from GitHub Actions with:

- `tag`: release tag such as `v0.1.0`
- `release_name`: optional display name
- `draft`: keep `true` for review before publishing
- `prerelease`: set `true` for prerelease builds

The workflow builds signed Tauri desktop bundles, `ai-switch-server`, `ai-switch-tsnet`, and `latest.json` updater metadata for GitHub Releases.
```

- [ ] **Step 2: Commit Task 4**

Run:

```powershell
git add README.md
git commit -m "docs: document release workflow"
```

---

### Task 5: Final Local Verification

**Files:**
- No new files.

**Interfaces:**
- Consumes: all previous tasks.
- Produces: local confidence before the first GitHub Actions run.

- [ ] **Step 1: Run manifest tests**

Run: `pnpm release:manifest:test`

Expected: PASS.

- [ ] **Step 2: Validate Tauri config JSON**

Run:

```powershell
node -e "JSON.parse(require('fs').readFileSync('src-tauri/tauri.conf.json','utf8')); console.log('valid')"
```

Expected: `valid`

- [ ] **Step 3: Run frontend checks**

Run: `pnpm typecheck`

Expected: PASS.

Run: `pnpm test:run`

Expected: PASS.

- [ ] **Step 4: Run Rust checks**

Run: `pnpm rust:check`

Expected: PASS.

Run: `pnpm rust:test`

Expected: PASS.

- [ ] **Step 5: Run Go sidecar tests**

Run:

```powershell
Push-Location sidecar\ai-switch-tsnet
go test ./...
Pop-Location
```

Expected: PASS.

- [ ] **Step 6: Inspect git status**

Run: `git status --short`

Expected: only the pre-existing unrelated `src-tauri/Cargo.toml` modification may remain unstaged.

- [ ] **Step 7: Report GitHub-only verification**

Tell the user that the real cross-platform Tauri packaging and release upload must be verified by running the manual **Release** workflow in GitHub Actions with `draft: true`.
