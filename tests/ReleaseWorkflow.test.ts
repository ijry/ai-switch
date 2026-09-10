import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

function readWorkflow() {
  return readFileSync(resolve(process.cwd(), ".github/workflows/release.yml"), "utf8");
}

function readPackageManagersWorkflow() {
  return readFileSync(resolve(process.cwd(), ".github/workflows/package-managers.yml"), "utf8");
}

describe("release workflow release notes", () => {
  const workflow = readWorkflow();

  it("takes the release notes from the tag commit message", () => {
    expect(workflow).toContain('git log -1 --format=%B "$GITHUB_SHA" > release-notes.md');
  });

  it("never asks GitHub to generate the notes", () => {
    // Generated notes for a release tag are only a compare link, and shipping
    // them left the updater manifest with a single line instead of a changelog.
    expect(workflow).not.toContain("generateReleaseNotes: true");
  });

  it("gives every release update a body file so the body is not cleared", () => {
    const bodyFileUses = workflow.match(/^\s+bodyFile: release-(notes|body)\.md$/gm) ?? [];

    expect(bodyFileUses).toHaveLength(2);
  });

  it("keeps the download table out of the notes the client renders", () => {
    // The published body gets a download table prepended, but latest.json feeds
    // the in-app changelog, which splits the tag message on its own separator.
    expect(workflow.match(/^\s+bodyFile: release-body\.md$/gm) ?? []).toHaveLength(1);
    expect(workflow).toContain("--output release-body.md");
    expect(workflow).not.toContain("--notes-file release-body.md");
  });

  it("feeds the notes file into the updater manifest", () => {
    expect(workflow).toContain("--notes-file release-notes.md");
  });

  it("passes the notes from prepare to publish instead of reading back the release body", () => {
    expect(workflow).toContain("release_notes: ${{ steps.notes.outputs.body }}");
    expect(workflow).toContain("RELEASE_NOTES: ${{ needs.prepare.outputs.release_notes }}");
  });
});

describe("standalone server release archive", () => {
  const workflow = readWorkflow();

  it("checks standalone features before building desktop bundles", () => {
    const checks = workflow.indexOf("- name: Check standalone server");
    const bundle = workflow.indexOf("- name: Build Tauri bundle");

    expect(checks).toBeGreaterThan(-1);
    expect(bundle).toBeGreaterThan(checks);
    expect(workflow.slice(checks, bundle)).toContain("run: pnpm server:check");
  });

  it("stages the frontend bundle next to the server binary", () => {
    // resolve_static_dir() only accepts a directory that holds index.html, and
    // the desktop bundle gets one from tauri.conf.json's "../dist": "web/" map.
    // The server archive has no such layer: ship it bare and the browser lands
    // on a JSON 404 instead of the UI.
    expect(workflow).toContain('Copy-Item dist (Join-Path $serverStage "web") -Recurse');
    expect(workflow).toContain('Compress-Archive -Path "$serverStage/*"');
  });

  it("fails the build instead of shipping a server archive without the UI", () => {
    expect(workflow).toContain('Join-Path $serverStage "web/index.html"');
    expect(workflow).toContain('throw "Server bundle is missing web/index.html"');
  });

  it("ships the tailscale sidecar under the name the server looks for", () => {
    // tailscale_sidecar.rs falls back to a sibling `ai-switch-tsnet[.exe]`, not
    // the target-triple name the build produces.
    expect(workflow).toContain('"ai-switch-tsnet$ExeSuffix"');
  });

  it("builds and stages a Linux ARM64 standalone server archive", () => {
    expect(workflow).toContain("rustup target add aarch64-unknown-linux-gnu");
    expect(workflow).toContain('-Platform "linux-aarch64"');
  });

  it("builds the Linux ARM64 sidecar from its Go module directory", () => {
    const arm64Step = workflow.slice(
      workflow.indexOf("- name: Build Linux ARM64 standalone server"),
      workflow.indexOf("- name: Stage release assets"),
    );

    expect(arm64Step).toContain("pushd sidecar/ai-switch-tsnet");
    expect(arm64Step).toContain('-o "../../$SIDECAR_ARM64_BIN" .');
  });

  it("returns to the repository root before building the ARM64 Rust server", () => {
    const arm64Step = workflow.slice(
      workflow.indexOf("- name: Build Linux ARM64 standalone server"),
      workflow.indexOf("- name: Stage release assets"),
    );
    const sidecarBuild = arm64Step.indexOf("go build");
    const restoreDirectory = arm64Step.indexOf("popd");
    const cargoBuild = arm64Step.indexOf("cargo build");

    expect(restoreDirectory).toBeGreaterThan(sidecarBuild);
    expect(cargoBuild).toBeGreaterThan(restoreDirectory);
    expect(arm64Step).toContain("cd src-tauri");
  });
});

describe("release asset list", () => {
  const workflow = readWorkflow();

  it("names the bundle assets through the staging script", () => {
    // GitHub sorts the asset list by name and folds all but the first few away,
    // so the installers only stay visible while the script decides their names.
    expect(workflow).toContain("node scripts/stage-release-assets.mjs");
    // create-package-manifests.mjs resolves the Homebrew and WinGet installers
    // by the updater platform token, so the staged names have to carry it.
    expect(workflow).toContain("--platform $env:UPDATER_PLATFORM");
    expect(workflow).toContain("--version $env:APP_VERSION");
  });

  it("deletes the signature files only after the manifest inlined them", () => {
    const manifest = workflow.indexOf("scripts/create-updater-manifest.mjs");
    const verify = workflow.indexOf("scripts/verify-updater-signatures.mjs");
    const deletion = workflow.indexOf("find release-assets -name '*.sig' -delete");

    expect(manifest).toBeGreaterThan(-1);
    expect(deletion).toBeGreaterThan(manifest);
    expect(deletion).toBeGreaterThan(verify);
  });
});

describe("package manager handoff", () => {
  const workflow = readWorkflow();

  it("dispatches package-managers.yml instead of relying on the release event", () => {
    // `release: published` does not fire for a release created with the job's own
    // GITHUB_TOKEN, so v0.8.1 published without any package-manager run. The
    // dispatch is the only thing that starts one; the tag input is what tells it
    // which release to package.
    expect(workflow).toContain("gh workflow run package-managers.yml");
    expect(workflow).toContain('-f tag="$TAG"');
  });

  it("dispatches the tag so the manifests come from the released commit", () => {
    expect(workflow).toContain('--ref "$TAG"');
  });

  it("grants the publish job the actions: write the dispatch needs", () => {
    // The workflow-level default is contents: write alone, and a dispatch with
    // that token is a 403 — which would only surface as a red release run.
    expect(workflow).toMatch(/permissions:\n\s+contents: write\n\s+actions: write/);
  });

  it("cannot fail an already published release", () => {
    // The package managers live in their own workflow precisely so a rejected
    // submission never marks the release itself as failed; a handoff that throws
    // here would give that back.
    expect(workflow).toContain("::warning::Could not dispatch package-managers.yml");
  });
});

describe("AppImage Wayland patch", () => {
  const script = readFileSync(
    resolve(process.cwd(), "src-tauri/scripts/patch-appimage-wayland.sh"),
    "utf8",
  );

  it("finds the architecture-specific linuxdeploy AppImage Tauri caches", () => {
    // Tauri 2.x saves linuxdeploy as `linuxdeploy-<arch>.AppImage` in its
    // tools cache, not as a file named plain `linuxdeploy`. Looking for the
    // old name leaves the AppDir patched but unpacked, and fails the release.
    expect(script).toContain('LINUXDEPLOY_ARCH="${HOST_TRIPLE:-$(uname -m)}"');
    expect(script).toContain('LINUXDEPLOY_ARCH="${LINUXDEPLOY_ARCH%%-*}"');
    expect(script.match(/-name "linuxdeploy-\${LINUXDEPLOY_ARCH}\.AppImage"/g) ?? []).toHaveLength(2);
  });

  it("re-packs with supported linuxdeploy options and extraction mode", () => {
    // GitHub runners cannot mount AppImages, and Tauri itself runs linuxdeploy
    // in extraction mode. Re-running linuxdeploy also redeploys dependencies,
    // so the removed Wayland libraries must be excluded explicitly.
    expect(script).toContain('OUTPUT="$OUTPUT" ARCH="$LINUXDEPLOY_ARCH" APPIMAGE_EXTRACT_AND_RUN=1 "$LINUXDEPLOY"');
    expect(script).toContain('--appimage-extract-and-run');
    expect(script).toContain('--exclude-library "libwayland*.so*"');
    expect(script).not.toContain("--deploy-library-path");
  });
});

describe("package manager dry run", () => {
  const workflow = readPackageManagersWorkflow();

  it("does not require the tokens it never reads", () => {
    // A dry run stops before both pushing steps, so gating it on the secrets made
    // the rehearsal impossible until the tap repository and the winget fork
    // existed — while the rehearsal is exactly what tells you whether an ad-hoc
    // signed cask is still installable at all.
    expect(workflow).toContain('elif [[ -z "$HOMEBREW_TAP_TOKEN" && "$DRY_RUN" != "true" ]]; then');
    expect(workflow).toContain('elif [[ -z "$WINGET_TOKEN" && "$DRY_RUN" != "true" ]]; then');
    expect(workflow).toContain("DRY_RUN: ${{ inputs.dry_run }}");
  });

  it("keeps every outward-facing step behind the dry-run switch", () => {
    // Two things reach another repository: the cask push and the winget PR. Both
    // have to stay gated, or a rehearsal would publish. They are gated in
    // different places — the cask push is a whole job, so its condition sits in
    // the job-level `if` beside the publishable/homebrew checks, while the winget
    // submission is one step inside a job that also does the dry-run reporting.
    // So assert the gate per target instead of counting one particular syntax,
    // which is what this test used to do: moving the cask push behind a job-level
    // `if` failed it while leaving the rehearsal every bit as safe.
    const homebrewPushJob = workflow.slice(
      workflow.indexOf("\n  homebrew-push:"),
      workflow.indexOf("\n  winget:"),
    );
    expect(homebrewPushJob).toMatch(/&& !inputs\.dry_run/);

    const wingetSubmit = workflow.slice(
      workflow.indexOf("- name: Submit the manifest to winget-pkgs"),
    );
    expect(wingetSubmit).toContain("if: ${{ !inputs.dry_run }}");

    // And nothing else: a third step that pushes would have to gate itself too.
    expect(workflow.match(/!inputs\.dry_run/g) ?? []).toHaveLength(2);
  });
});
