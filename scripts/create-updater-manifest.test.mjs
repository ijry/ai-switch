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

    await writeFile(path.join(winDir, "ai-switch-0.1.0-windows-x86_64-setup.exe"), "binary");
    await writeFile(path.join(winDir, "ai-switch-0.1.0-windows-x86_64-setup.exe.sig"), "win-signature\n");
    await writeFile(path.join(linuxDir, "ai-switch-0.1.0-linux-x86_64.AppImage"), "binary");
    await writeFile(path.join(linuxDir, "ai-switch-0.1.0-linux-x86_64.AppImage.sig"), "linux-signature\n");

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
      "https://github.com/ijry/ai-switch/releases/download/v0.1.0/ai-switch-0.1.0-windows-x86_64-setup.exe",
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("includes release notes from a file", async () => {
  const root = await mkdtemp(path.join(tmpdir(), "ai-switch-release-"));

  try {
    const winDir = path.join(root, "windows-x86_64");
    await mkdir(winDir, { recursive: true });
    await writeFile(path.join(winDir, "ai-switch-0.1.0-windows-x86_64-setup.exe"), "binary");
    await writeFile(path.join(winDir, "ai-switch-0.1.0-windows-x86_64-setup.exe.sig"), "signature\n");

    const notesFile = path.join(root, "release-notes.md");
    await writeFile(notesFile, "## 修复\n\n- 显示更新日志\n\n");
    const output = path.join(root, "latest.json");

    await createManifest({
      assetsDir: root,
      tag: "v0.1.0",
      repo: "ijry/ai-switch",
      output,
      notesFile,
    });

    const manifest = JSON.parse(await readFile(output, "utf8"));
    assert.equal(manifest.notes, "## 修复\n\n- 显示更新日志");
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("fails when a platform directory has no signed updater asset", async () => {
  const root = await mkdtemp(path.join(tmpdir(), "ai-switch-release-"));

  try {
    const winDir = path.join(root, "windows-x86_64");
    await mkdir(winDir, { recursive: true });
    await writeFile(path.join(winDir, "ai-switch-0.1.0-windows-x86_64-setup.exe"), "binary");

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

test("prefers macOS updater archive over signed installer image", async () => {
  const root = await mkdtemp(path.join(tmpdir(), "ai-switch-release-"));

  try {
    const macDir = path.join(root, "darwin-aarch64");
    await mkdir(macDir, { recursive: true });

    // Staging strips the spaces Tauri puts in bundle names, but the URL builder
    // has to keep escaping whatever it is handed.
    await writeFile(path.join(macDir, "AI Switch.app.tar.gz"), "archive");
    await writeFile(path.join(macDir, "AI Switch.app.tar.gz.sig"), "archive-signature\n");
    await writeFile(path.join(macDir, "AI Switch.dmg"), "dmg");
    await writeFile(path.join(macDir, "AI Switch.dmg.sig"), "dmg-signature\n");

    const output = path.join(root, "latest.json");
    await createManifest({
      assetsDir: root,
      tag: "v0.1.0",
      repo: "ijry/ai-switch",
      output,
      pubDate: "2026-07-17T00:00:00.000Z",
    });

    const manifest = JSON.parse(await readFile(output, "utf8"));
    assert.equal(manifest.platforms["darwin-aarch64"].signature, "archive-signature");
    assert.equal(
      manifest.platforms["darwin-aarch64"].url,
      "https://github.com/ijry/ai-switch/releases/download/v0.1.0/AI%20Switch.app.tar.gz",
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
