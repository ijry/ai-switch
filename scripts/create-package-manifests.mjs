import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

// Renders package-manager metadata for a release that is *already* published on
// GitHub. `.github/workflows/package-managers.yml` hands over the release JSON
// straight from the API, so re-running an old tag renders exactly what that tag
// shipped instead of whatever the working tree happens to say today.

// The updater platform token is the one part of an asset name that survived the
// renaming this repo has done, so match on it rather than on the surrounding
// shape. Both of these have to resolve to the same Windows installer:
//   ai-switch_v0.8.0_windows-x86_64_AI-Switch_0.8.0_x64-setup.exe
//   ai-switch-0.8.0-windows-x86_64-setup.exe
//
// The Intel dmg is optional because every tag up to v0.8.2 shipped without one,
// and re-publishing an old tag has to keep working long after its build machines
// are gone. A missing one renders the arm-only cask those releases already have.
const INSTALLER_TARGETS = [
  { key: "macos", platform: "darwin-aarch64", extension: /\.dmg$/i },
  { key: "macosIntel", platform: "darwin-x86_64", extension: /\.dmg$/i, optional: true },
  { key: "windows", platform: "windows-x86_64", extension: /\.exe$/i },
];

// Updater-only payloads. A package manager that installed one of these would
// hand the user a nested archive instead of an app. `.sig` files are minisign
// signatures for those payloads and also share the platform token.
const NON_INSTALLER_PATTERNS = [
  /\.sig$/i,
  /\.app\.tar\.gz$/i,
  /\.nsis\.zip$/i,
  /^ai-switch-updater-/i,
];

// Identity the package managers publish under. These strings are contracts, not
// cosmetics: changing `caskToken` orphans everyone who ran `brew install`, and
// changing `wingetIdentifier` means a second package in winget-pkgs rather than
// a rename.
export const PACKAGE = {
  caskToken: "ai-switch",
  appBundle: "AI Switch.app",
  wingetIdentifier: "ijry.AISwitch",
  name: "AI Switch",
  desc: "Switch provider accounts and API routes for AI coding CLIs",
  homepage: "https://ijry.github.io/ai-switch/",
  bundleIdentifier: "io.xyito.ai-switch",
  // Every bit of local state, credentials included. Only ever removed by an
  // explicit `brew uninstall --zap`.
  dataDirectory: "~/.ai-switch",
};

export function versionFromTag(tag) {
  return String(tag).replace(/^v/i, "");
}

export function isPrereleaseTag(tag) {
  return /-(rc|beta|alpha)(\.\d+)?$/i.test(String(tag));
}

export function selectInstallerAssets(assets) {
  const selected = {};

  for (const target of INSTALLER_TARGETS) {
    const matches = assets.filter(
      (asset) =>
        !NON_INSTALLER_PATTERNS.some((pattern) => pattern.test(asset.name)) &&
        asset.name.includes(target.platform) &&
        target.extension.test(asset.name),
    );

    if (matches.length === 0) {
      if (target.optional) {
        continue;
      }
      throw new Error(`No ${target.platform} installer among the release assets`);
    }
    if (matches.length > 1) {
      const names = matches.map((asset) => asset.name).join(", ");
      throw new Error(`Ambiguous ${target.platform} installer: ${names}`);
    }

    selected[target.key] = matches[0];
  }

  return selected;
}

function parseApiDigest(asset) {
  // The releases API reports `digest: "sha256:<hex>"` for anything uploaded
  // since GitHub added asset digests, which saves pulling a 31 MB dmg down just
  // to hash it. Assets from older releases report null, so the caller has to be
  // able to fall back to hashing the download.
  const digest = asset?.digest;
  if (typeof digest !== "string") {
    return null;
  }
  const match = /^sha256:([0-9a-f]{64})$/i.exec(digest.trim());
  return match ? match[1].toLowerCase() : null;
}

export async function resolveSha256(asset, { fetchImpl = fetch } = {}) {
  const fromApi = parseApiDigest(asset);
  if (fromApi) {
    return { sha256: fromApi, source: "api" };
  }

  const url = asset?.browser_download_url;
  if (!url) {
    throw new Error(`Asset ${asset?.name ?? "<unnamed>"} has neither a digest nor a download URL`);
  }

  const response = await fetchImpl(url, { redirect: "follow" });
  if (!response.ok) {
    throw new Error(`Downloading ${asset.name} for hashing failed with HTTP ${response.status}`);
  }

  const hash = createHash("sha256");
  for await (const chunk of response.body) {
    hash.update(chunk);
  }

  return { sha256: hash.digest("hex"), source: "download" };
}

// Casks are expected to interpolate the version into the URL rather than pin a
// literal one, and both naming schemes repeat the version (the tag directory
// plus one or two copies inside the file name). Substituting every occurrence
// keeps the rendered stanza idiomatic without hard-coding either shape.
export function interpolateVersion(text, version) {
  if (!version) {
    throw new Error("Cannot interpolate an empty version");
  }
  return text.split(version).join("#{version}");
}

// The two macOS dmgs differ only in the updater platform's arch token, so one
// `url` stanza plus an `arch` stanza serves both — `arch` resolves to the token
// for the Mac running brew. Homebrew's own style cops push this way: nesting
// only `sha256`, or `version` plus `sha256`, in `on_arm`/`on_intel` blocks is an
// offense they autocorrect into exactly these stanzas.
const CASK_ARCH_TOKENS = { arm: "aarch64", intel: "x86_64" };

function interpolateArch(url, token) {
  return url.split(token).join("#{arch}");
}

// One template both dmgs agree on, or nothing. Guessing here would publish a
// cask whose Intel URL 404s, and the tap has no review step to catch that.
function archTemplatedUrl({ version, url, intelUrl }) {
  const fromArm = interpolateVersion(interpolateArch(url, CASK_ARCH_TOKENS.arm), version);
  const fromIntel = interpolateVersion(interpolateArch(intelUrl, CASK_ARCH_TOKENS.intel), version);

  if (fromArm !== fromIntel) {
    throw new Error(
      `The macOS installers do not share one arch-templated URL:\n  ${fromArm}\n  ${fromIntel}`,
    );
  }

  return fromArm;
}

function rubyString(value) {
  return `"${String(value).replace(/\\/g, "\\\\").replace(/"/g, '\\"')}"`;
}

export function renderHomebrewCask({ token = PACKAGE.caskToken, version, sha256, url, intel, repo }) {
  // `arch` leads its own stanza group ahead of `version`, and Cask/Sha256ArchOrder
  // wants arm before intel with the values aligned under the table hash style.
  const head = intel
    ? `  arch arm: ${rubyString(CASK_ARCH_TOKENS.arm)}, intel: ${rubyString(CASK_ARCH_TOKENS.intel)}

  version ${rubyString(version)}
  sha256 arm:   ${rubyString(sha256)},
         intel: ${rubyString(intel.sha256)}

  url ${rubyString(archTemplatedUrl({ version, url, intelUrl: intel.url }))},`
    : `  version ${rubyString(version)}
  sha256 ${rubyString(sha256)}

  url ${rubyString(interpolateVersion(url, version))},`;

  // Keeps the rationale in the tap rather than only here: the two branches
  // differ in whether an Intel Mac gets a dmg or a refusal, and that is the
  // first thing a reader of the cask will wonder about.
  const dependsOn = intel
    ? `  # Both architectures ship a native dmg, so this stanza's only remaining job
  # is to mark the cask macOS-only: casks can target Linux now, so brew style's
  # Homebrew/OSDependsOn wants the OS said out loud. No macos: floor is declared
  # on purpose — the version symbols Homebrew accepts get pruned as releases go
  # EOL, so a floor pinned today turns into a load error later.
  depends_on :macos`
    : `  # Only Apple Silicon has a dmg in this release, so an Intel Mac has to be
  # told rather than handed an arm64 bundle it cannot launch at all. No macos:
  # floor is declared on purpose — every arm64 Mac is already past it, and the
  # version symbols Homebrew accepts get pruned as releases go EOL. The bare
  # :macos marks the cask macOS-only without a floor, which is what brew style's
  # Homebrew/OSDependsOn asks for now that casks can also target Linux. Both go
  # in one stanza because the DSL takes the OS positionally alongside the
  # keyword arguments.
  depends_on :macos, arch: :arm64`;

  const zapTargets = [
    PACKAGE.dataDirectory,
    `~/Library/Caches/${PACKAGE.bundleIdentifier}`,
    `~/Library/HTTPStorages/${PACKAGE.bundleIdentifier}`,
    `~/Library/Preferences/${PACKAGE.bundleIdentifier}.plist`,
    `~/Library/Saved Application State/${PACKAGE.bundleIdentifier}.savedState`,
    `~/Library/WebKit/${PACKAGE.bundleIdentifier}`,
  ];

  return `# Generated by scripts/create-package-manifests.mjs from the ${repo} release.
# Edit the generator, not this file: the next release overwrites it.
cask ${rubyString(token)} do
${head}
      verified: ${rubyString(`github.com/${repo}/`)}
  name ${rubyString(PACKAGE.name)}
  desc ${rubyString(PACKAGE.desc)}
  homepage ${rubyString(PACKAGE.homepage)}

  livecheck do
    url :url
    strategy :github_latest
  end

  # The app carries a Tauri updater that checks hourly and replaces its own
  # bundle, so what Homebrew recorded goes stale without Homebrew doing
  # anything. auto_updates is what keeps \`brew upgrade\` from fighting it.
  auto_updates true
${dependsOn}

  app ${rubyString(PACKAGE.appBundle)}

  # CI bundles macOS with signingIdentity "-", so the app is ad-hoc signed and
  # never notarized. Gatekeeper answers a quarantined ad-hoc bundle with "is
  # damaged and can't be opened", which is the dialog the install guide walks
  # users through by hand. Clearing the flag on the copy Homebrew just placed is
  # what makes this a one-step install; it touches that path only and changes no
  # system-wide setting.
  #
  # must_succeed: false because xattr exits non-zero when the attribute is
  # already gone, and a Homebrew that did not quarantine the download must not
  # turn into a failed install.
  #
  # The padding after args: is what brew style wants: its Layout/HashAlignment is
  # configured for table style, so the values line up rather than the keys.
  postflight do
    system_command "/usr/bin/xattr",
                   args:         ["-dr", "com.apple.quarantine", "#{appdir}/${PACKAGE.appBundle}"],
                   must_succeed: false
  end

  # Credentials live in ~/.ai-switch, so this only ever runs for someone who
  # asked for \`brew uninstall --zap\`.
  zap trash: [
${zapTargets.map((target) => `    ${rubyString(target)},`).join("\n")}
  ]
end
`;
}

// winget-releaser re-finds the installer itself by regex-matching the release
// assets, so an unanchored pattern is a second, looser selector that can
// disagree with the one above. Anchoring on the name this script already
// resolved makes the two agree by construction.
export function installersRegexFor(assetName) {
  return `^${assetName.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}$`;
}

export async function createPackageManifests({
  releaseFile,
  tag,
  repo,
  outDir,
  fetchImpl = fetch,
}) {
  const release = JSON.parse(await readFile(releaseFile, "utf8"));
  const assets = Array.isArray(release.assets) ? release.assets : [];
  if (assets.length === 0) {
    throw new Error(`Release ${tag} has no assets to publish`);
  }

  const version = versionFromTag(tag);
  const selected = selectInstallerAssets(assets);
  const installers = {};

  for (const [key, asset] of Object.entries(selected)) {
    const { sha256, source } = await resolveSha256(asset, { fetchImpl });
    installers[key] = {
      name: asset.name,
      url: asset.browser_download_url,
      sha256,
      checksumSource: source,
    };
  }

  const cask = renderHomebrewCask({
    version,
    sha256: installers.macos.sha256,
    url: installers.macos.url,
    intel: installers.macosIntel && {
      sha256: installers.macosIntel.sha256,
      url: installers.macosIntel.url,
    },
    repo,
  });
  // Kept relative and POSIX-style so the value is the same whether the render
  // ran on a CI runner or a Windows checkout.
  const caskRelativePath = `homebrew/Casks/${PACKAGE.caskToken}.rb`;
  const caskPath = path.join(outDir, ...caskRelativePath.split("/"));
  await mkdir(path.dirname(caskPath), { recursive: true });
  await writeFile(caskPath, cask);

  const summary = {
    tag,
    version,
    repo,
    prerelease: isPrereleaseTag(tag),
    installers,
    homebrew: { token: PACKAGE.caskToken, caskPath: caskRelativePath },
    winget: {
      identifier: PACKAGE.wingetIdentifier,
      version,
      installerName: installers.windows.name,
      installerUrl: installers.windows.url,
      installerSha256: installers.windows.sha256,
      installersRegex: installersRegexFor(installers.windows.name),
    },
  };

  const summaryPath = path.join(outDir, "summary.json");
  await writeFile(summaryPath, `${JSON.stringify(summary, null, 2)}\n`);
  return summary;
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
    releaseFile: required(args, "release"),
    tag: required(args, "tag"),
    repo: required(args, "repo"),
    outDir: required(args, "out-dir"),
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
  createPackageManifests(parseArgs(process.argv.slice(2)))
    .then((summary) => {
      for (const [key, installer] of Object.entries(summary.installers)) {
        console.log(`${key}: ${installer.name} (sha256 from ${installer.checksumSource})`);
      }
      console.log(`Cask: ${summary.homebrew.caskPath}`);
      console.log(`WinGet: ${summary.winget.identifier} ${summary.winget.version}`);
    })
    .catch((error) => {
      console.error(error instanceof Error ? error.message : String(error));
      process.exitCode = 1;
    });
}

