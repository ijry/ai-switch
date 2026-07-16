import { mkdir, readdir, readFile, stat, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const updaterAssetPreference = new Map([
  ["windows-x86_64", [/\.exe$/i, /\.msi$/i]],
  ["windows-aarch64", [/\.exe$/i, /\.msi$/i]],
  ["darwin-x86_64", [/\.dmg$/i]],
  ["darwin-aarch64", [/\.dmg$/i]],
  ["linux-x86_64", [/\.AppImage$/i, /\.deb$/i]],
  ["linux-aarch64", [/\.AppImage$/i, /\.deb$/i]],
]);

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
    tag: required(args, "tag"),
    repo: required(args, "repo"),
    output: required(args, "output"),
    pubDate: args.get("pub-date") ?? new Date().toISOString(),
  };
}

function required(args, key) {
  const value = args.get(key);
  if (!value) {
    throw new Error(`Missing --${key}`);
  }
  return value;
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

function platformFromDirectory(assetsDir, filePath) {
  const relative = path.relative(assetsDir, filePath);
  const [platform] = relative.split(path.sep);
  return platform;
}

function releaseUrl(repo, tag, assetName) {
  return `https://github.com/${repo}/releases/download/${encodeURIComponent(tag)}/${encodeURIComponent(assetName)}`;
}

function versionFromTag(tag) {
  return tag.replace(/^v/i, "");
}

function pickSignedAsset(platform, signedAssets) {
  const preferences = updaterAssetPreference.get(platform) ?? [];

  for (const pattern of preferences) {
    const match = signedAssets.find((asset) => pattern.test(asset.assetPath));
    if (match) {
      return match;
    }
  }

  return signedAssets[0];
}

export async function createManifest({ assetsDir, tag, repo, output, pubDate = new Date().toISOString() }) {
  const rootStat = await stat(assetsDir);
  if (!rootStat.isDirectory()) {
    throw new Error(`Assets path is not a directory: ${assetsDir}`);
  }

  const files = await listFilesRecursive(assetsDir);
  const signatures = files.filter((file) => file.endsWith(".sig"));
  const signedByPlatform = new Map();

  for (const signaturePath of signatures) {
    const assetPath = signaturePath.slice(0, -".sig".length);
    if (!files.includes(assetPath)) {
      throw new Error(`Signature has no matching asset: ${signaturePath}`);
    }

    const platform = platformFromDirectory(assetsDir, signaturePath);
    if (!platform) {
      throw new Error(`Signed asset must be inside a platform directory: ${signaturePath}`);
    }

    const entries = signedByPlatform.get(platform) ?? [];
    entries.push({ assetPath, signaturePath });
    signedByPlatform.set(platform, entries);
  }

  const platforms = {};
  const platformDirectories = (await readdir(assetsDir, { withFileTypes: true }))
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name)
    .sort();

  for (const platform of platformDirectories) {
    const signedAssets = signedByPlatform.get(platform) ?? [];
    const picked = pickSignedAsset(platform, signedAssets);
    if (!picked) {
      throw new Error(`No signed updater asset found for ${platform}`);
    }

    const signature = (await readFile(picked.signaturePath, "utf8")).trim();
    const assetName = path.basename(picked.assetPath);
    platforms[platform] = {
      signature,
      url: releaseUrl(repo, tag, assetName),
    };
  }

  const manifest = {
    version: versionFromTag(tag),
    notes: "",
    pub_date: pubDate,
    platforms,
  };

  await mkdir(path.dirname(output), { recursive: true });
  await writeFile(output, `${JSON.stringify(manifest, null, 2)}\n`);
  return manifest;
}

const isMain = process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1]);

if (isMain) {
  createManifest(parseArgs(process.argv.slice(2))).catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  });
}
