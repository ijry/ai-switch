import assert from "node:assert/strict";
import { readdir, readFile, stat } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

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
    manifest: required(args, "manifest"),
    tauriConfig: required(args, "tauri-config"),
  };
}

function required(args, key) {
  const value = args.get(key);
  if (!value) {
    throw new Error(`Missing --${key}`);
  }
  return value;
}

function signerKeyId(minisignText) {
  const signatureLine = minisignText.split(/\r?\n/)[1]?.trim();
  if (!signatureLine) {
    throw new Error("Missing minisign signature payload");
  }

  const payload = Buffer.from(signatureLine, "base64");
  if (payload.length < 10 || payload.subarray(0, 2).toString("ascii") !== "ED") {
    throw new Error("Invalid minisign signature payload");
  }

  return payload.subarray(2, 10).reverse().toString("hex").toUpperCase();
}

function updaterKeyId(pubkey) {
  const minisignText = Buffer.from(pubkey, "base64").toString("utf8");
  const publicKeyLine = minisignText.split(/\r?\n/)[1]?.trim();
  if (!publicKeyLine) {
    throw new Error("Missing minisign public key payload");
  }

  const payload = Buffer.from(publicKeyLine, "base64");
  if (payload.length < 10 || payload.subarray(0, 2).toString("ascii") !== "Ed") {
    throw new Error("Invalid minisign public key payload");
  }

  return payload.subarray(2, 10).reverse().toString("hex").toUpperCase();
}

export async function verifyUpdaterSignatures({ manifest, tauriConfig }) {
  const config = JSON.parse(await readFile(tauriConfig, "utf8"));
  const pubkey = config.plugins?.updater?.pubkey;
  if (typeof pubkey !== "string") {
    throw new Error("Tauri updater public key is missing");
  }

  const expectedKeyId = updaterKeyId(pubkey);
  const manifestStat = await stat(manifest);
  if (!manifestStat.isFile()) {
    throw new Error(`Updater manifest is not a file: ${manifest}`);
  }

  const updaterManifest = JSON.parse(await readFile(manifest, "utf8"));
  const platforms = Object.entries(updaterManifest.platforms ?? {});
  if (platforms.length === 0) {
    throw new Error("Updater manifest has no platform signatures");
  }

  for (const [platform, entry] of platforms) {
    const signature = entry?.signature;
    if (typeof signature !== "string") {
      throw new Error(`Updater manifest signature is missing for ${platform}`);
    }
    const signerId = signerKeyId(Buffer.from(signature, "base64").toString("utf8"));
    if (signerId !== expectedKeyId) {
      throw new Error(
        `Updater signature key mismatch for ${platform}: expected ${expectedKeyId}, received ${signerId}`,
      );
    }
  }

  return { keyId: expectedKeyId, signatureCount: platforms.length };
}

const isMain = process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1]);

if (isMain) {
  verifyUpdaterSignatures(parseArgs(process.argv.slice(2)))
    .then(({ keyId, signatureCount }) => {
      console.log(`Verified ${signatureCount} updater signature(s) with key ${keyId}.`);
    })
    .catch((error) => {
      console.error(error instanceof Error ? error.message : String(error));
      process.exitCode = 1;
    });
}
