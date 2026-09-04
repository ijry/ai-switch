import assert from "node:assert/strict";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { test } from "node:test";
import { pathToFileURL } from "node:url";

const moduleUrl = pathToFileURL(path.resolve("scripts/create-release-body.mjs")).href;
const { createReleaseBody } = await import(moduleUrl);

const SEPARATOR = "-".repeat(29);
const NOTES = `release: v0.8.0\n\n### 新功能\n\n- 中文条目\n\n${SEPARATOR}\n\n### Features\n\n- English entry`;

async function assetsFixture(files) {
  const root = await mkdtemp(path.join(tmpdir(), "ai-switch-body-"));

  for (const relative of files) {
    const target = path.join(root, relative);
    await mkdir(path.dirname(target), { recursive: true });
    await writeFile(target, "asset");
  }

  await writeFile(path.join(root, "release-notes.md"), `${NOTES}\n`);
  return root;
}

test("puts a labelled download table above the changelog", async () => {
  const root = await assetsFixture([
    "windows-x86_64/ai-switch-0.8.0-windows-x86_64-setup.exe",
    "windows-x86_64/ai-switch-server_v0.8.0_windows-x86_64.zip",
    "windows-x86_64/ai-switch-tsnet_v0.8.0_windows-x86_64.zip",
    "darwin-aarch64/ai-switch-0.8.0-darwin-aarch64.dmg",
    "darwin-aarch64/ai-switch-updater-0.8.0-darwin-aarch64.app.tar.gz",
    "darwin-aarch64/ai-switch-server_v0.8.0_darwin-aarch64.zip",
    "linux-x86_64/ai-switch-0.8.0-linux-x86_64.AppImage",
    "linux-x86_64/ai-switch-0.8.0-linux-x86_64.deb",
    "linux-x86_64/ai-switch-server_v0.8.0_linux-x86_64.zip",
    "latest.json",
  ]);

  try {
    const output = path.join(root, "release-body.md");
    await createReleaseBody({
      assetsDir: root,
      tag: "v0.8.0",
      repo: "ijry/ai-switch",
      notesFile: path.join(root, "release-notes.md"),
      output,
    });

    const body = await readFile(output, "utf8");
    const rows = body
      .split("\n")
      .filter((line) => line.startsWith("| ") && !line.startsWith("| ---"))
      .slice(1);

    assert.deepEqual(
      rows.map((row) => row.split(" | ")[0].replace("| ", "")),
      ["Windows (x64)", "macOS (Apple Silicon)", "Linux (x64)"],
    );
    assert.match(
      rows[0],
      /\[ai-switch-0\.8\.0-windows-x86_64-setup\.exe\]\(https:\/\/github\.com\/ijry\/ai-switch\/releases\/download\/v0\.8\.0\/ai-switch-0\.8\.0-windows-x86_64-setup\.exe\)/,
    );
    // The AppImage and the .deb share the Linux row; neither is the "main" one.
    assert.match(rows[2], /AppImage\).+·.+\.deb\)/);
    // Updater-only payloads and the sidecar stay out of the table.
    assert.doesNotMatch(body, /app\.tar\.gz|tsnet/);
    assert.match(body, /独立服务器.+Standalone server: \[Windows \(x64\)\]/);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("keeps the tag message intact so the client can still split it", async () => {
  const root = await assetsFixture(["windows-x86_64/ai-switch-0.8.0-windows-x86_64-setup.exe"]);

  try {
    const output = path.join(root, "release-body.md");
    await createReleaseBody({
      assetsDir: root,
      tag: "v0.8.0",
      repo: "ijry/ai-switch",
      notesFile: path.join(root, "release-notes.md"),
      output,
    });

    const body = await readFile(output, "utf8");
    assert.ok(body.endsWith(`${NOTES}\n`), "the changelog must survive verbatim");
    assert.equal(body.split("\n").filter((line) => line === SEPARATOR).length, 1);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("falls back to the changelog alone when a build produced no installer", async () => {
  const root = await assetsFixture(["darwin-aarch64/ai-switch-updater-0.8.0-darwin-aarch64.app.tar.gz"]);

  try {
    const output = path.join(root, "release-body.md");
    const body = await createReleaseBody({
      assetsDir: root,
      tag: "v0.8.0",
      repo: "ijry/ai-switch",
      notesFile: path.join(root, "release-notes.md"),
      output,
    });

    assert.equal(body, `${NOTES}\n`);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
