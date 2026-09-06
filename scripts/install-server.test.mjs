import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";

const installerPath = new URL("./install-server.sh", import.meta.url);
const servicePath = new URL("../deploy/ai-switch-server.service", import.meta.url);
const workflowPath = new URL("../.github/workflows/release.yml", import.meta.url);

test("Linux installer provisions a protected systemd service on shared port", () => {
  assert.equal(fs.existsSync(installerPath), true, "scripts/install-server.sh should exist");
  assert.equal(fs.existsSync(servicePath), true, "deploy/ai-switch-server.service should exist");
  const installer = fs.readFileSync(installerPath, "utf8");
  const service = fs.readFileSync(servicePath, "utf8");
  for (const text of [
    "set -euo pipefail",
    "curl -fsSL",
    "/opt/ai-switch",
    "useradd --system",
    "AI_SWITCH_TOKEN",
    "19527",
    "systemctl enable --now ai-switch-server.service",
  ]) {
    assert.ok(installer.includes(text), `installer should contain ${text}`);
  }
  assert.match(installer, /Keeping existing environment/);
  assert.match(installer, /run_root test -r "\$ENV_FILE"/);
  assert.match(installer, /run_root sed -n 's\/\^AI_SWITCH_TOKEN=\/\/p' "\$ENV_FILE"/);
  assert.doesNotMatch(installer, /AI_SWITCH_ALLOW_INSECURE_HTTP=/);
  assert.doesNotMatch(installer, /aarch64|arm64/);
  assert.doesNotMatch(installer, /nginx|certbot|ufw/i);

  assert.match(service, /^User=ai-switch$/m);
  assert.match(service, /^Environment=HOME=\/var\/lib\/ai-switch$/m);
  assert.match(service, /^EnvironmentFile=\/etc\/ai-switch\/server\.env$/m);
  assert.match(service, /^Restart=on-failure$/m);
  assert.match(installer, /Environment=HOME=\/var\/lib\/ai-switch/);
});

test("only the Linux server archive receives installer and service unit", () => {
  const workflow = fs.readFileSync(workflowPath, "utf8");
  assert.match(workflow, /matrix\.label[^\n]+Linux/);
  assert.match(workflow, /Copy-Item scripts\/install-server\.sh/);
  assert.match(workflow, /Copy-Item deploy\/ai-switch-server\.service/);

  const packageJson = fs.readFileSync(new URL("../package.json", import.meta.url), "utf8");
  assert.match(packageJson, /release:manifest:test[^\n]*install-server\.test\.mjs/);
});