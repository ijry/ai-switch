import assert from "node:assert/strict";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { Readable } from "node:stream";
import { test } from "node:test";
import path from "node:path";
import { pathToFileURL } from "node:url";

const moduleUrl = pathToFileURL(path.resolve("scripts/create-package-manifests.mjs")).href;
const { resolveSha256, selectInstallerAssets, versionFromTag, isPrereleaseTag, interpolateVersion, renderHomebrewCask, installersRegexFor, createPackageManifests } =
  await import(moduleUrl);

function asset(name, digest = null) {
  return {
    name,
    digest,
    browser_download_url: `https://github.com/ijry/ai-switch/releases/download/v0.8.0/${name}`,
  };
}

// The exact asset list GitHub reported for v0.8.0. Anything that stops selecting
// one installer per platform out of this list breaks re-publishing old tags.
const V0_8_0_ASSETS = [
  "ai-switch-server_v0.8.0_darwin-aarch64.zip",
  "ai-switch-server_v0.8.0_linux-x86_64.zip",
  "ai-switch-server_v0.8.0_windows-x86_64.zip",
  "ai-switch-tsnet_v0.8.0_darwin-aarch64.zip",
  "ai-switch-tsnet_v0.8.0_linux-x86_64.zip",
  "ai-switch-tsnet_v0.8.0_windows-x86_64.zip",
  "ai-switch_v0.8.0_darwin-aarch64_AI-Switch.app.tar.gz",
  "ai-switch_v0.8.0_darwin-aarch64_AI-Switch.app.tar.gz.sig",
  "ai-switch_v0.8.0_darwin-aarch64_AI-Switch_0.8.0_aarch64.dmg",
  "ai-switch_v0.8.0_linux-x86_64_AI-Switch_0.8.0_amd64.AppImage",
  "ai-switch_v0.8.0_linux-x86_64_AI-Switch_0.8.0_amd64.AppImage.sig",
  "ai-switch_v0.8.0_linux-x86_64_AI-Switch_0.8.0_amd64.deb",
  "ai-switch_v0.8.0_linux-x86_64_AI-Switch_0.8.0_amd64.deb.sig",
  "ai-switch_v0.8.0_linux-x86_64_control.tar.gz",
  "ai-switch_v0.8.0_linux-x86_64_data.tar.gz",
  "ai-switch_v0.8.0_windows-x86_64_AI-Switch_0.8.0_x64-setup.exe",
  "ai-switch_v0.8.0_windows-x86_64_AI-Switch_0.8.0_x64-setup.exe.sig",
  "latest.json",
].map((name) => asset(name));

// A release built on both macOS runners. Every tag up to v0.8.2 predates the
// Intel job, so both shapes have to keep rendering a cask.
const DUAL_ARCH_ASSETS = [
  "ai-switch-0.9.0-darwin-aarch64.dmg",
  "ai-switch-0.9.0-darwin-x86_64.dmg",
  "ai-switch-0.9.0-linux-x86_64.AppImage",
  "ai-switch-0.9.0-linux-x86_64.deb",
  "ai-switch-0.9.0-windows-x86_64-setup.exe",
  "ai-switch-server_v0.9.0_darwin-aarch64.zip",
  "ai-switch-server_v0.9.0_darwin-x86_64.zip",
  "ai-switch-tsnet_v0.9.0_darwin-x86_64.zip",
  "ai-switch-updater-0.9.0-darwin-aarch64.app.tar.gz",
  "ai-switch-updater-0.9.0-darwin-aarch64.app.tar.gz.sig",
  "ai-switch-updater-0.9.0-darwin-x86_64.app.tar.gz",
  "ai-switch-updater-0.9.0-darwin-x86_64.app.tar.gz.sig",
  "ai-switch-updater-0.9.0-windows-x86_64.nsis.zip",
  "ai-switch-updater-0.9.0-windows-x86_64.nsis.zip.sig",
  "latest.json",
].map((name) => asset(name));

const DMG_BASE = "https://github.com/ijry/ai-switch/releases/download/v0.9.0/ai-switch-0.9.0-darwin";

// Stanza order per the Cask Cookbook, which brew style's Cask/StanzaOrder cop
// enforces. `arch` leads its own group ahead of `version` when it is present.
const CASK_STANZA_ORDER = [
  "version ",
  "sha256 ",
  "url ",
  "name ",
  "desc ",
  "homepage ",
  "livecheck do",
  "auto_updates ",
  "depends_on :macos",
  "app ",
  "postflight do",
  "zap ",
];

function assertStanzaOrder(cask, order) {
  let cursor = -1;
  for (const stanza of order) {
    const index = cask.indexOf(`\n  ${stanza}`);
    assert.ok(index > cursor, `${stanza} is out of order`);
    cursor = index;
  }
}

test("selects one installer per platform from the legacy asset naming", () => {
  const selected = selectInstallerAssets(V0_8_0_ASSETS);

  assert.equal(selected.macos.name, "ai-switch_v0.8.0_darwin-aarch64_AI-Switch_0.8.0_aarch64.dmg");
  assert.equal(
    selected.windows.name,
    "ai-switch_v0.8.0_windows-x86_64_AI-Switch_0.8.0_x64-setup.exe",
  );
  // No Intel job existed yet, and that has to stay publishable rather than error.
  assert.equal(selected.macosIntel, undefined);
});

test("selects both macOS dmgs once the Intel job publishes one", () => {
  const selected = selectInstallerAssets(DUAL_ARCH_ASSETS);

  assert.equal(selected.macos.name, "ai-switch-0.9.0-darwin-aarch64.dmg");
  assert.equal(selected.macosIntel.name, "ai-switch-0.9.0-darwin-x86_64.dmg");
  assert.equal(selected.windows.name, "ai-switch-0.9.0-windows-x86_64-setup.exe");
});

test("selects one installer per platform from the current asset naming", () => {
  const selected = selectInstallerAssets(
    [
      "ai-switch-0.9.0-darwin-aarch64.dmg",
      "ai-switch-0.9.0-linux-x86_64.AppImage",
      "ai-switch-0.9.0-linux-x86_64.deb",
      "ai-switch-0.9.0-windows-x86_64-setup.exe",
      "ai-switch-server_v0.9.0_windows-x86_64.zip",
      "ai-switch-tsnet_v0.9.0_windows-x86_64.zip",
      "ai-switch-updater-0.9.0-darwin-aarch64.app.tar.gz",
      "ai-switch-updater-0.9.0-darwin-aarch64.app.tar.gz.sig",
      "ai-switch-updater-0.9.0-windows-x86_64.nsis.zip",
      "ai-switch-updater-0.9.0-windows-x86_64.nsis.zip.sig",
      "latest.json",
    ].map((name) => asset(name)),
  );

  assert.equal(selected.macos.name, "ai-switch-0.9.0-darwin-aarch64.dmg");
  assert.equal(selected.windows.name, "ai-switch-0.9.0-windows-x86_64-setup.exe");
});

test("refuses a release that is missing a platform installer", () => {
  const assets = V0_8_0_ASSETS.filter((entry) => !entry.name.endsWith(".dmg"));

  assert.throws(() => selectInstallerAssets(assets), /No darwin-aarch64 installer/);
});

test("refuses two candidates for the same platform instead of guessing", () => {
  const assets = [...V0_8_0_ASSETS, asset("ai-switch-0.8.0-darwin-aarch64.dmg")];

  assert.throws(() => selectInstallerAssets(assets), /Ambiguous darwin-aarch64 installer/);
});

test("takes the checksum from the API digest without downloading", async () => {
  const digest = "8857d20e9990b38d43654c82b4dd5f9fb1c089507623b171d1c4ed2f40d72bd5";
  const result = await resolveSha256(asset("ai-switch-0.8.0-darwin-aarch64.dmg", `sha256:${digest}`), {
    fetchImpl: () => {
      throw new Error("must not download when the API already reported a digest");
    },
  });

  assert.deepEqual(result, { sha256: digest, source: "api" });
});

test("hashes the download when the API reports no digest", async () => {
  // Releases published before GitHub added asset digests report null, which is
  // exactly the case when re-publishing an old tag to a package manager.
  const result = await resolveSha256(asset("ai-switch-0.8.0-darwin-aarch64.dmg"), {
    fetchImpl: async () => ({ ok: true, status: 200, body: Readable.from(["ai-", "switch"]) }),
  });

  assert.equal(result.source, "download");
  // sha256("ai-switch")
  assert.equal(result.sha256, "67da1d95dd0e663872cfa9064ba5dae3e47ac138c4644dc95403344c415191d1");
});

test("reports a failed download instead of publishing a wrong checksum", async () => {
  await assert.rejects(
    () =>
      resolveSha256(asset("ai-switch-0.8.0-darwin-aarch64.dmg"), {
        fetchImpl: async () => ({ ok: false, status: 404, body: Readable.from([]) }),
      }),
    /HTTP 404/,
  );
});

test("derives the package version and prerelease flag from the tag", () => {
  assert.equal(versionFromTag("v0.8.0"), "0.8.0");
  assert.equal(versionFromTag("v0.9.0-rc.1"), "0.9.0-rc.1");
  assert.equal(isPrereleaseTag("v0.8.0"), false);
  assert.equal(isPrereleaseTag("v0.9.0-rc.1"), true);
  assert.equal(isPrereleaseTag("v0.9.0-beta"), true);
});

test("interpolates every copy of the version the asset name repeats", () => {
  assert.equal(
    interpolateVersion(
      "https://github.com/ijry/ai-switch/releases/download/v0.8.0/ai-switch_v0.8.0_darwin-aarch64_AI-Switch_0.8.0_aarch64.dmg",
      "0.8.0",
    ),
    "https://github.com/ijry/ai-switch/releases/download/v#{version}/ai-switch_v#{version}_darwin-aarch64_AI-Switch_#{version}_aarch64.dmg",
  );
});

test("renders a cask whose stanzas match the release it was built from", () => {
  const cask = renderHomebrewCask({
    version: "0.8.0",
    sha256: "8857d20e9990b38d43654c82b4dd5f9fb1c089507623b171d1c4ed2f40d72bd5",
    url: "https://github.com/ijry/ai-switch/releases/download/v0.8.0/ai-switch-0.8.0-darwin-aarch64.dmg",
    repo: "ijry/ai-switch",
  });

  assert.match(cask, /^cask "ai-switch" do$/m);
  assert.match(cask, /^ {2}version "0\.8\.0"$/m);
  assert.match(cask, /^ {2}sha256 "8857d20e[0-9a-f]{56}"$/m);
  assert.match(
    cask,
    /^ {2}url "https:\/\/github\.com\/ijry\/ai-switch\/releases\/download\/v#\{version\}\/ai-switch-#\{version\}-darwin-aarch64\.dmg",$/m,
  );
  assert.match(cask, /^ {6}verified: "github\.com\/ijry\/ai-switch\/"$/m);
  assert.match(cask, /^ {2}app "AI Switch\.app"$/m);
  // Only Apple Silicon has a dmg in this release, so an Intel Mac must be told
  // rather than handed an arm64 bundle. The bare :macos is what brew style's
  // Homebrew/OSDependsOn asks for now that casks can also target Linux.
  assert.match(cask, /^ {2}depends_on :macos, arch: :arm64$/m);
  assert.match(cask, /^ {2}auto_updates true$/m);
  assert.match(cask, /com\.apple\.quarantine/);
  // brew style runs Layout/HashAlignment in table style, so the values of a
  // multi-line hash have to line up, not the keys.
  assert.match(cask, /^ {19}args: {9}\["-dr", "com\.apple\.quarantine", "#\{appdir\}\/AI Switch\.app"\],\n {19}must_succeed: false$/m);
  assert.match(cask, /"~\/\.ai-switch",/);
  assert.equal(cask.endsWith("end\n"), true);
});

test("renders one arch-templated cask when both macOS dmgs are published", () => {
  const cask = renderHomebrewCask({
    version: "0.9.0",
    sha256: "a".repeat(64),
    url: `${DMG_BASE}-aarch64.dmg`,
    intel: { sha256: "b".repeat(64), url: `${DMG_BASE}-x86_64.dmg` },
    repo: "ijry/ai-switch",
  });

  assert.match(cask, /^ {2}arch arm: "aarch64", intel: "x86_64"$/m);
  // Cask/Sha256ArchOrder wants arm before intel, and the values line up under
  // Layout/HashAlignment's table style.
  assert.match(cask, /^ {2}sha256 arm: {3}"a{64}",\n {9}intel: "b{64}"$/m);
  // One url stanza for both: #{arch} resolves to the token for the running Mac.
  assert.match(
    cask,
    /^ {2}url "https:\/\/github\.com\/ijry\/ai-switch\/releases\/download\/v#\{version\}\/ai-switch-#\{version\}-darwin-#\{arch\}\.dmg",$/m,
  );
  assert.match(cask, /^ {6}verified: "github\.com\/ijry\/ai-switch\/"$/m);
  // An Intel Mac now has something to install, so the refusal has to go with it.
  assert.match(cask, /^ {2}depends_on :macos$/m);
  assert.doesNotMatch(cask, /arch: :arm64/);
  assert.equal(cask.endsWith("end\n"), true);
});

test("refuses to guess a URL template when the two dmgs are not named alike", () => {
  // The legacy scheme spelled the arm arch twice and the Intel one as `x64` in
  // its second half, so no single #{arch} substitution reproduces both names.
  assert.throws(
    () =>
      renderHomebrewCask({
        version: "0.9.0",
        sha256: "a".repeat(64),
        url: "https://github.com/ijry/ai-switch/releases/download/v0.9.0/ai-switch_v0.9.0_darwin-aarch64_AI-Switch_0.9.0_aarch64.dmg",
        intel: {
          sha256: "b".repeat(64),
          url: "https://github.com/ijry/ai-switch/releases/download/v0.9.0/ai-switch_v0.9.0_darwin-x86_64_AI-Switch_0.9.0_x64.dmg",
        },
        repo: "ijry/ai-switch",
      }),
    /do not share one arch-templated URL/,
  );
});

test("keeps the cask stanzas in the order brew style expects", () => {
  const cask = renderHomebrewCask({
    version: "0.8.0",
    sha256: "a".repeat(64),
    url: "https://github.com/ijry/ai-switch/releases/download/v0.8.0/ai-switch-0.8.0-darwin-aarch64.dmg",
    repo: "ijry/ai-switch",
  });

  assertStanzaOrder(cask, CASK_STANZA_ORDER);
});

test("keeps the dual-arch cask stanzas in order too, with arch leading", () => {
  const cask = renderHomebrewCask({
    version: "0.9.0",
    sha256: "a".repeat(64),
    url: `${DMG_BASE}-aarch64.dmg`,
    intel: { sha256: "b".repeat(64), url: `${DMG_BASE}-x86_64.dmg` },
    repo: "ijry/ai-switch",
  });

  assertStanzaOrder(cask, ["arch ", ...CASK_STANZA_ORDER]);
});

test("keeps the cask description inside the length Homebrew audits for", () => {
  const cask = renderHomebrewCask({
    version: "0.8.0",
    sha256: "a".repeat(64),
    url: "https://github.com/ijry/ai-switch/releases/download/v0.8.0/ai-switch-0.8.0-darwin-aarch64.dmg",
    repo: "ijry/ai-switch",
  });

  const desc = /^ {2}desc "(.+)"$/m.exec(cask)?.[1];
  assert.ok(desc, "the cask has no desc stanza");
  assert.ok(desc.length <= 80, `desc is ${desc.length} characters`);
  // Homebrew rejects a description that opens with an article or the token.
  assert.doesNotMatch(desc, /^(a|an|the|ai switch)\b/i);
  assert.doesNotMatch(desc, /\.$/);
});

test("anchors the winget installer regex on the resolved asset name", () => {
  const regex = installersRegexFor("ai-switch_v0.8.0_windows-x86_64_AI-Switch_0.8.0_x64-setup.exe");

  assert.equal(
    regex,
    "^ai-switch_v0\\.8\\.0_windows-x86_64_AI-Switch_0\\.8\\.0_x64-setup\\.exe$",
  );
  // The dots must not stay wildcards, or a second `.exe` asset could match too.
  const pattern = new RegExp(regex);
  assert.equal(pattern.test("ai-switch_v0.8.0_windows-x86_64_AI-Switch_0.8.0_x64-setup.exe"), true);
  assert.equal(pattern.test("ai-switch_v0X8Y0_windows-x86_64_AI-Switch_0.8.0_x64-setup.exe"), false);
  assert.equal(
    pattern.test("prefix-ai-switch_v0.8.0_windows-x86_64_AI-Switch_0.8.0_x64-setup.exe"),
    false,
  );
});

test("writes the cask and a summary the workflow can read back", async () => {
  const root = await mkdtemp(path.join(tmpdir(), "ai-switch-packages-"));

  try {
    const releaseFile = path.join(root, "release.json");
    await writeFile(
      releaseFile,
      JSON.stringify({
        tag_name: "v0.8.0",
        draft: false,
        prerelease: false,
        assets: V0_8_0_ASSETS.map((entry) => ({
          ...entry,
          digest: `sha256:${"b".repeat(64)}`,
        })),
      }),
    );

    const outDir = path.join(root, "out");
    const summary = await createPackageManifests({
      releaseFile,
      tag: "v0.8.0",
      repo: "ijry/ai-switch",
      outDir,
      fetchImpl: () => {
        throw new Error("must not download when every asset reports a digest");
      },
    });

    assert.equal(summary.version, "0.8.0");
    assert.equal(summary.winget.identifier, "ijry.AISwitch");
    assert.equal(
      summary.winget.installerName,
      "ai-switch_v0.8.0_windows-x86_64_AI-Switch_0.8.0_x64-setup.exe",
    );
    assert.equal(summary.installers.macos.sha256, "b".repeat(64));

    const cask = await readFile(path.join(outDir, "homebrew", "Casks", "ai-switch.rb"), "utf8");
    assert.match(cask, /^ {2}version "0\.8\.0"$/m);

    const written = JSON.parse(await readFile(path.join(outDir, "summary.json"), "utf8"));
    assert.deepEqual(written, summary);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("carries both macOS checksums from the release into the cask", async () => {
  const root = await mkdtemp(path.join(tmpdir(), "ai-switch-packages-"));

  try {
    const releaseFile = path.join(root, "release.json");
    // Distinct digests on purpose: brew style flags a cask whose two
    // architectures report the same sha256, and so would a release that somehow
    // published one dmg twice.
    await writeFile(
      releaseFile,
      JSON.stringify({
        tag_name: "v0.9.0",
        draft: false,
        prerelease: false,
        assets: DUAL_ARCH_ASSETS.map((entry) => ({
          ...entry,
          digest: `sha256:${(entry.name.includes("x86_64") ? "c" : "d").repeat(64)}`,
        })),
      }),
    );

    const outDir = path.join(root, "out");
    const summary = await createPackageManifests({
      releaseFile,
      tag: "v0.9.0",
      repo: "ijry/ai-switch",
      outDir,
      fetchImpl: () => {
        throw new Error("must not download when every asset reports a digest");
      },
    });

    assert.equal(summary.installers.macos.sha256, "d".repeat(64));
    assert.equal(summary.installers.macosIntel.sha256, "c".repeat(64));

    const cask = await readFile(path.join(outDir, "homebrew", "Casks", "ai-switch.rb"), "utf8");
    assert.match(cask, /^ {2}arch arm: "aarch64", intel: "x86_64"$/m);
    assert.match(cask, /^ {2}sha256 arm: {3}"d{64}",\n {9}intel: "c{64}"$/m);
    assert.match(cask, /-darwin-#\{arch\}\.dmg/);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

