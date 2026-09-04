import { readdir, readFile, mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

// The release body renders above the asset list, and GitHub folds that list
// after the first few entries. Even with the installers named to sort first,
// a labelled table is what makes them findable: "which of these 12 files do I
// want" is not a question a visitor should have to answer.
//
// This block only goes into the GitHub release body. The updater manifest keeps
// the tag message verbatim, because the in-app changelog splits it on its own
// separator to pick a language.

// Windows leads because that is where most users are, not because of the name.
const PLATFORMS = [
  { id: "windows-x86_64", label: "Windows (x64)" },
  { id: "windows-aarch64", label: "Windows (ARM64)" },
  { id: "darwin-aarch64", label: "macOS (Apple Silicon)" },
  { id: "darwin-x86_64", label: "macOS (Intel)" },
  { id: "linux-x86_64", label: "Linux (x64)" },
  { id: "linux-aarch64", label: "Linux (ARM64)" },
];

const INSTALLER_EXTENSIONS = [/\.exe$/i, /\.msi$/i, /\.dmg$/i, /\.AppImage$/i, /\.deb$/i, /\.rpm$/i];

function installerRank(fileName) {
  return INSTALLER_EXTENSIONS.findIndex((pattern) => pattern.test(fileName));
}

function releaseUrl(repo, tag, assetName) {
  return `https://github.com/${repo}/releases/download/${encodeURIComponent(tag)}/${encodeURIComponent(assetName)}`;
}

function link(repo, tag, assetName, text = assetName) {
  return `[${text}](${releaseUrl(repo, tag, assetName)})`;
}

async function readPlatforms(assetsDir) {
  const entries = await readdir(assetsDir, { withFileTypes: true });
  const directories = entries.filter((entry) => entry.isDirectory()).map((entry) => entry.name);
  const known = PLATFORMS.filter(({ id }) => directories.includes(id));
  // A new build target should show up in the table instead of vanishing.
  const unknown = directories
    .filter((id) => !PLATFORMS.some((platform) => platform.id === id))
    .sort()
    .map((id) => ({ id, label: id }));

  const platforms = [];
  for (const platform of [...known, ...unknown]) {
    const files = await readdir(path.join(assetsDir, platform.id));
    platforms.push({
      ...platform,
      installers: files
        .filter((file) => installerRank(file) >= 0)
        .sort((left, right) => installerRank(left) - installerRank(right) || left.localeCompare(right)),
      server: files.find((file) => /^ai-switch-server[-_]/i.test(file)),
    });
  }

  return platforms;
}

export async function createReleaseBody({ assetsDir, tag, repo, notesFile, output }) {
  const notes = notesFile ? (await readFile(notesFile, "utf8")).trim() : "";
  const platforms = await readPlatforms(assetsDir);
  const installable = platforms.filter(({ installers }) => installers.length > 0);
  const servers = platforms.filter(({ server }) => server);

  const lines = [];
  if (installable.length > 0) {
    lines.push("**下载 · Downloads**", "", "| 平台 · Platform | 安装包 · Installer |", "| --- | --- |");
    for (const { label, installers } of installable) {
      const downloads = installers.map((file) => link(repo, tag, file)).join(" · ");
      lines.push(`| ${label} | ${downloads} |`);
    }
  }

  const blocks = lines.length > 0 ? [lines.join("\n")] : [];
  if (servers.length > 0) {
    const downloads = servers.map(({ label, server }) => link(repo, tag, server, label)).join(" · ");
    blocks.push(`独立服务器（解压即用，浏览器访问）· Standalone server: ${downloads}`);
  }
  if (blocks.length > 0 && notes) {
    blocks.push("---");
  }
  if (notes) {
    blocks.push(notes);
  }

  const body = `${blocks.join("\n\n")}\n`;
  await mkdir(path.dirname(path.resolve(output)), { recursive: true });
  await writeFile(output, body);
  return body;
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
    assetsDir: required(args, "assets-dir"),
    tag: required(args, "tag"),
    repo: required(args, "repo"),
    output: required(args, "output"),
    notesFile: args.get("notes-file"),
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
  createReleaseBody(parseArgs(process.argv.slice(2)))
    .then((body) => {
      console.log(body.split("\n").slice(0, 12).join("\n"));
    })
    .catch((error) => {
      console.error(error instanceof Error ? error.message : String(error));
      process.exitCode = 1;
    });
}
