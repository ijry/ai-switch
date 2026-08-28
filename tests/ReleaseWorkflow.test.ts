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
