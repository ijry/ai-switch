import assert from "node:assert/strict";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { test } from "node:test";
import { pathToFileURL } from "node:url";

const moduleUrl = pathToFileURL(path.resolve("scripts/verify-updater-signatures.mjs")).href;
const { verifyUpdaterSignatures } = await import(moduleUrl);

const matchingPublicKey =
  "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEREODg3RThFMjMzMUI0QkYKUldTL3RERWpqbjZJM2VqZDlBZjFtZlZ4enNzSlFmUVJhdEk3bjB4aG1aM0lxbUE5WmlmL1I5b2gK";
const matchingSignature = [
  "untrusted comment: signature from minisign secret key",
  "RUS/tDEjjn6I3bC45JFnto5XNKSiaGvSHc7q30wwZmZ3IqmaA9Zif/R9oh",
].join("\n");
const mismatchedSignature = [
  "untrusted comment: signature from minisign secret key",
  Buffer.concat([Buffer.from("ED"), Buffer.from("0102030405060708", "hex"), Buffer.alloc(64)]).toString("base64"),
].join("\n");

async function writeTauriConfig(root, pubkey = matchingPublicKey) {
  const configPath = path.join(root, "tauri.conf.json");
  await writeFile(configPath, JSON.stringify({ plugins: { updater: { pubkey } } }));
  return configPath;
}

test("verifies updater signatures made with the configured public key", async () => {
  const root = await mkdtemp(path.join(tmpdir(), "ai-switch-updater-signatures-"));

  try {
    const assetsDir = path.join(root, "assets", "windows-x86_64");
    await mkdir(assetsDir, { recursive: true });
    await writeFile(path.join(assetsDir, "AI-Switch.exe.sig"), matchingSignature);

    const result = await verifyUpdaterSignatures({
      assetsDir: path.dirname(assetsDir),
      tauriConfig: await writeTauriConfig(root),
    });

    assert.deepEqual(result, { keyId: "DD887E8E2331B4BF", signatureCount: 1 });
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("rejects updater signatures made with another key", async () => {
  const root = await mkdtemp(path.join(tmpdir(), "ai-switch-updater-signatures-"));

  try {
    const assetsDir = path.join(root, "assets", "windows-x86_64");
    await mkdir(assetsDir, { recursive: true });
    await writeFile(path.join(assetsDir, "AI-Switch.exe.sig"), mismatchedSignature);

    await assert.rejects(
      async () =>
        verifyUpdaterSignatures({
          assetsDir: path.dirname(assetsDir),
          tauriConfig: await writeTauriConfig(root),
        }),
      /Updater signature key mismatch/,
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
