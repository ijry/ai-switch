import test from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";

const packageJsonPath = new URL("../package.json", import.meta.url);
const srcTauriPath = new URL("../src-tauri", import.meta.url);

function cargoTree(args) {
  const result = spawnSync("cargo", ["tree", ...args], {
    cwd: fs.realpathSync(srcTauriPath),
    encoding: "utf8",
  });
  assert.equal(result.status, 0, result.stderr);
  return result.stdout;
}

test("standalone Linux server does not link WebKitGTK", () => {
  const packageJson = JSON.parse(fs.readFileSync(packageJsonPath, "utf8"));
  for (const scriptName of ["server:build", "server:build:release", "server:check"]) {
    assert.match(
      packageJson.scripts[scriptName],
      /--no-default-features --features standalone-server/,
      `${scriptName} should disable desktop features`,
    );
  }

  const tree = cargoTree([
    "--target",
    "x86_64-unknown-linux-gnu",
    "--no-default-features",
    "--features",
    "standalone-server",
    "-e",
    "features",
  ]);
  assert.doesNotMatch(tree, /webkit2gtk/i, "standalone server should not enable WebKitGTK");
});

test("default Linux build still enables the desktop webview", () => {
  const tree = cargoTree([
    "--target",
    "x86_64-unknown-linux-gnu",
    "-e",
    "features",
  ]);
  assert.match(tree, /webkit2gtk/i, "desktop build should retain WebKitGTK");
});
