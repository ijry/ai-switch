import assert from "node:assert/strict";
import { mkdir, mkdtemp, readdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { test } from "node:test";
import { pathToFileURL } from "node:url";

const moduleUrl = pathToFileURL(path.resolve("scripts/stage-release-assets.mjs")).href;
const { publishedAssetName, stageReleaseAssets } = await import(moduleUrl);

async function bundleFixture(files) {
  const root = await mkdtemp(path.join(tmpdir(), "ai-switch-stage-"));

  for (const [relative, contents] of Object.entries(files)) {
    const target = path.join(root, relative);
    await mkdir(path.dirname(target), { recursive: true });
    await writeFile(target, contents);
  }

  return root;
}

test("installers sort before the server and sidecar archives", () => {
  const published = [
    publishedAssetName("AI-Switch_0.8.0_x64-setup.exe", { platform: "windows-x86_64", version: "0.8.0" }).name,
    publishedAssetName("AI-Switch_0.8.0_aarch64.dmg", { platform: "darwin-aarch64", version: "0.8.0" }).name,
    publishedAssetName("AI-Switch_0.8.0_amd64.AppImage", { platform: "linux-x86_64", version: "0.8.0" }).name,
  ];
  const extras = ["ai-switch-server_v0.8.0_windows-x86_64.zip", "ai-switch-tsnet_v0.8.0_windows-x86_64.zip"];

  // GitHub lists assets by name and hides the tail behind "Show all N assets",
  // so every installer has to sort ahead of the extra downloads.
  for (const installer of published) {
    for (const extra of extras) {
      assert.ok(installer < extra, `${installer} must sort before ${extra}`);
      assert.ok(
        installer.localeCompare(extra, "en") < 0,
        `${installer} must also sort before ${extra} when punctuation is ignored`,
      );
    }
  }
});

test("names each bundle kind after the platform it installs on", () => {
  const options = { platform: "darwin-aarch64", version: "0.8.0" };

  assert.deepEqual(publishedAssetName("AI-Switch_0.8.0_aarch64.dmg", options), {
    kind: "installer",
    name: "ai-switch-0.8.0-darwin-aarch64.dmg",
  });
  assert.deepEqual(publishedAssetName("AI-Switch.app.tar.gz", options), {
    kind: "updater",
    name: "ai-switch-updater-0.8.0-darwin-aarch64.app.tar.gz",
  });
  assert.deepEqual(publishedAssetName("AI-Switch.app.tar.gz.sig", options), {
    kind: "signature",
    name: "ai-switch-updater-0.8.0-darwin-aarch64.app.tar.gz.sig",
  });
  // create-package-manifests.mjs resolves the Homebrew and WinGet installers by
  // this exact shape, so the platform token has to stay in the name.
  assert.equal(
    publishedAssetName("AI-Switch_0.8.0_x64-setup.exe", { ...options, platform: "windows-x86_64" }).name,
    "ai-switch-0.8.0-windows-x86_64-setup.exe",
  );
});

test("signatures keep pairing with the asset they sign", async () => {
  const root = await bundleFixture({
    "nsis/AI-Switch_0.8.0_x64-setup.exe": "installer",
    "nsis/AI-Switch_0.8.0_x64-setup.exe.sig": "signature\n",
  });
  const dest = path.join(root, "staged");

  try {
    const { names } = await stageReleaseAssets({
      bundleDir: root,
      dest,
      platform: "windows-x86_64",
      version: "0.8.0",
    });

    // create-updater-manifest.mjs pairs `<asset>.sig` with `<asset>` by name.
    assert.deepEqual(names, [
      "ai-switch-0.8.0-windows-x86_64-setup.exe",
      "ai-switch-0.8.0-windows-x86_64-setup.exe.sig",
    ]);
    assert.deepEqual((await readdir(dest)).sort(), names);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("leaves the archives the .deb is assembled from out of the release", async () => {
  const root = await bundleFixture({
    "appimage/AI-Switch_0.8.0_amd64.AppImage": "appimage",
    "appimage/AI-Switch_0.8.0_amd64.AppImage.sig": "signature\n",
    "deb/ai-switch_0.8.0_amd64/control.tar.gz": "deb-internals",
    "deb/ai-switch_0.8.0_amd64/data.tar.gz": "deb-payload",
    "deb/ai-switch_0.8.0_amd64/debian-binary": "2.0\n",
    "deb/ai-switch_0.8.0_amd64.deb": "package",
    "deb/ai-switch_0.8.0_amd64.deb.sig": "signature\n",
  });

  try {
    const { names, counts } = await stageReleaseAssets({
      bundleDir: root,
      dest: path.join(root, "staged"),
      platform: "linux-x86_64",
      version: "0.8.0",
    });

    assert.deepEqual(names, [
      "ai-switch-0.8.0-linux-x86_64.AppImage",
      "ai-switch-0.8.0-linux-x86_64.AppImage.sig",
      "ai-switch-0.8.0-linux-x86_64.deb",
      "ai-switch-0.8.0-linux-x86_64.deb.sig",
    ]);
    assert.equal(counts.installer, 2);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("never digs into the built .app for assets", async () => {
  const root = await bundleFixture({
    "macos/AI-Switch.app/Contents/Resources/skill-packages/ai-switch.core.deb": "not-an-asset",
    "macos/AI-Switch.app.tar.gz": "archive",
    "macos/AI-Switch.app.tar.gz.sig": "signature\n",
    "dmg/AI-Switch_0.8.0_aarch64.dmg": "image",
  });

  try {
    const { names } = await stageReleaseAssets({
      bundleDir: root,
      dest: path.join(root, "staged"),
      platform: "darwin-aarch64",
      version: "0.8.0",
    });

    assert.deepEqual(names, [
      "ai-switch-0.8.0-darwin-aarch64.dmg",
      "ai-switch-updater-0.8.0-darwin-aarch64.app.tar.gz",
      "ai-switch-updater-0.8.0-darwin-aarch64.app.tar.gz.sig",
    ]);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("refuses to stage a bundle without an installer or a signature", async () => {
  const noInstaller = await bundleFixture({ "macos/AI-Switch.app.tar.gz": "archive" });
  const noSignature = await bundleFixture({ "nsis/AI-Switch_0.8.0_x64-setup.exe": "installer" });
  const options = { platform: "windows-x86_64", version: "0.8.0" };

  try {
    await assert.rejects(
      () => stageReleaseAssets({ ...options, bundleDir: noInstaller, dest: path.join(noInstaller, "staged") }),
      /No installer bundle found/,
    );
    await assert.rejects(
      () => stageReleaseAssets({ ...options, bundleDir: noSignature, dest: path.join(noSignature, "staged") }),
      /No updater signature found/,
    );
  } finally {
    await rm(noInstaller, { recursive: true, force: true });
    await rm(noSignature, { recursive: true, force: true });
  }
});
