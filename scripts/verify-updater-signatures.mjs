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
    assetsDir: required(args, "assets-dir"),
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

export async function verifyUpdaterSignatures({ assetsDir, tauriConfig }) {
  const config = JSON.parse(await readFile(tauriConfig, "utf8"));
  const pubkey = config.plugins?.updater?.pubkey;
  if (typeof pubkey !== "string") {
    throw new Error("Tauri updater public key is missing");
  }

  const expectedKeyId = updaterKeyId(pubkey);
  const rootStat = await stat(assetsDir);
  if (!rootStat.isDirectory()) {
    throw new Error(`Assets path is not a directory: ${assetsDir}`);
  }

  const signatureFiles = (await listFilesRecursive(assetsDir)).filter((file) => file.endsWith(".sig"));
  if (signatureFiles.length === 0) {
    throw new Error("No updater signature files found");
  }

  for (const signatureFile of signatureFiles) {
    const signerId = signerKeyId(await readFile(signatureFile, "utf8"));
    if (signerId !== expectedKeyId) {
      throw new Error(
        `Updater signature key mismatch for ${path.relative(assetsDir, signatureFile)}: expected ${expectedKeyId}, received ${signerId}`,
      );
    }
  }

  return { keyId: expectedKeyId, signatureCount: signatureFiles.length };
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
