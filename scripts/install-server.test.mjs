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
  assert.match(installer, /ALLOW_INSECURE_HTTP="\$\{AI_SWITCH_ALLOW_INSECURE_HTTP:-1\}"/);
  assert.match(installer, /AI_SWITCH_ALLOW_INSECURE_HTTP=%s/);
  assert.match(installer, /AI_SWITCH_PORT must be a number between 1 and 65535/);
  assert.match(installer, /systemctl is-active --quiet ai-switch-server\.service/);
  assert.match(installer, /systemctl --no-pager --full status ai-switch-server\.service/);
  assert.match(installer, /Panel URL:/);
  assert.match(installer, /grep -q "not found"/);
  assert.match(installer, /apt-get install -y libwebkit2gtk-4\.1-0/);
  assert.match(installer, /systemctl stop ai-switch-server\.service/);
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

test("standalone server documentation does not require WebKitGTK", () => {
  const zhDoc = fs.readFileSync(
    new URL("../docs-site/docs/deploy/standalone-server.md", import.meta.url),
    "utf8",
  );
  const enDoc = fs.readFileSync(
    new URL("../docs-site/docs/en/deploy/standalone-server.md", import.meta.url),
    "utf8",
  );

  assert.match(zhDoc, /不依赖 WebKitGTK/);
  assert.doesNotMatch(zhDoc, /仍依赖 WebKitGTK/);
  assert.match(enDoc, /does not require WebKitGTK/);
  assert.doesNotMatch(enDoc, /still depends on the WebKitGTK/);
});
