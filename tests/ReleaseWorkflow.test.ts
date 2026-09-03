import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

function readWorkflow() {
  return readFileSync(resolve(process.cwd(), ".github/workflows/release.yml"), "utf8");
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

  it("writes the same notes into every release update so the body is not cleared", () => {
    const bodyFileUses = workflow.match(/^\s+bodyFile: release-notes\.md$/gm) ?? [];

    expect(bodyFileUses).toHaveLength(2);
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
