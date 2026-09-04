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
    expect(workflow).toContain('"ai-switch-tsnet$env:EXE_SUFFIX"');
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
    // Two steps reach another repository: the cask push and the winget PR. Both
    // have to stay gated, or a rehearsal would publish.
    expect(workflow.match(/^\s+if: \$\{\{ !inputs\.dry_run \}\}$/gm) ?? []).toHaveLength(2);
  });
});
