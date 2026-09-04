import { copyFile, mkdir, readdir } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

// GitHub sorts release assets by name and folds everything past the first few
// behind "Show all N assets", so the published names decide what a visitor sees.
// The old `ai-switch_<tag>_<platform>_<original>` scheme lost that race: the
// tag's leading `v` sorts after the `s`/`t` of `ai-switch-server_…` and
// `ai-switch-tsnet_…`, which pushed the Windows installer and the macOS dmg —
// the two files almost everybody comes for — below the fold. Putting the version
// digits straight after `ai-switch-` sorts the installers first under both plain
// byte order and the punctuation-insensitive collation GitHub actually uses.
//
// The platform half stays the updater platform id (`windows-x86_64`,
// `darwin-aarch64`, …) rather than a friendlier `windows-x64`:
// create-package-manifests.mjs picks the Homebrew and WinGet installers by that
// token, and the release body carries the human-readable platform labels.

const INSTALLER_RULES = [
  { pattern: /-setup\.exe$/i, suffix: "-setup.exe" },
  { pattern: /\.exe$/i, suffix: "-setup.exe" },
  { pattern: /\.msi$/i, suffix: ".msi" },
  { pattern: /\.dmg$/i, suffix: ".dmg" },
  { pattern: /\.AppImage$/i, suffix: ".AppImage" },
  { pattern: /\.deb$/i, suffix: ".deb" },
  { pattern: /\.rpm$/i, suffix: ".rpm" },
];

// Payloads only the updater ever downloads, and only through `latest.json`.
// `-updater-` sorts them behind the extra zips instead of between the installers.
const UPDATER_RULES = [
  { pattern: /\.app\.tar\.gz$/i, suffix: ".app.tar.gz" },
  { pattern: /\.nsis\.zip$/i, suffix: ".nsis.zip" },
];

// `bundle/deb/<pkg>/` keeps the archives the .deb is assembled from. They match
// a plain `.tar.gz` filter, which is how ~37 MB of duplicate payload used to
// ship as two more release assets.
const IGNORED_NAMES = new Set(["control.tar.gz", "data.tar.gz", "debian-binary"]);

export function publishedAssetName(fileName, { platform, version }) {
  const isSignature = /\.sig$/i.test(fileName);
  const payload = isSignature ? fileName.slice(0, -".sig".length) : fileName;

  if (IGNORED_NAMES.has(payload)) {
    return null;
  }

  for (const [kind, rules, stem] of [
    ["installer", INSTALLER_RULES, "ai-switch"],
    ["updater", UPDATER_RULES, "ai-switch-updater"],
  ]) {
    const rule = rules.find(({ pattern }) => pattern.test(payload));
    if (!rule) {
      continue;
    }

    const name = `${stem}-${version}-${platform}${rule.suffix}`;
    return isSignature ? { kind: "signature", name: `${name}.sig` } : { kind, name };
  }

  return null;
}

async function listBundleFiles(root) {
  const entries = await readdir(root, { withFileTypes: true });
  const files = [];

  for (const entry of entries) {
    const entryPath = path.join(root, entry.name);
    if (entry.isDirectory()) {
      // The built `.app` carries the whole frontend and every bundled resource;
      // only its sibling `.app.tar.gz` is a release asset.
      if (entry.name.toLowerCase().endsWith(".app")) {
        continue;
      }
      files.push(...(await listBundleFiles(entryPath)));
    } else if (entry.isFile()) {
      files.push(entryPath);
    }
  }

  return files;
}

export async function stageReleaseAssets({ bundleDir, dest, platform, version }) {
  const files = await listBundleFiles(bundleDir);
  const staged = new Map();
  const counts = { installer: 0, updater: 0, signature: 0 };

  for (const file of files) {
    const mapped = publishedAssetName(path.basename(file), { platform, version });
    if (!mapped) {
      continue;
    }

    const previous = staged.get(mapped.name);
    if (previous) {
      throw new Error(`Two bundle files claim the asset name ${mapped.name}: ${previous} and ${file}`);
    }

    staged.set(mapped.name, file);
    counts[mapped.kind] += 1;
  }

  if (counts.installer === 0) {
    throw new Error(`No installer bundle found under ${bundleDir}`);
  }
  if (counts.signature === 0) {
    // Without a signature the manifest step fails much later, after another
    // platform may already have published.
    throw new Error(`No updater signature found under ${bundleDir}`);
  }

  await mkdir(dest, { recursive: true });
  const names = [...staged.keys()].sort();
  for (const name of names) {
    await copyFile(staged.get(name), path.join(dest, name));
  }

  return { names, counts };
}

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
    bundleDir: required(args, "bundle-dir"),
    dest: required(args, "dest"),
    platform: required(args, "platform"),
    version: required(args, "version"),
  };
}

function required(args, key) {
  const value = args.get(key);
  if (!value) {
    throw new Error(`Missing --${key}`);
  }
  return value;
}

const isMain = process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1]);

if (isMain) {
  stageReleaseAssets(parseArgs(process.argv.slice(2)))
    .then(({ names }) => {
      console.log(`Staged ${names.length} release asset(s):\n${names.join("\n")}`);
    })
    .catch((error) => {
      console.error(error instanceof Error ? error.message : String(error));
      process.exitCode = 1;
    });
}
