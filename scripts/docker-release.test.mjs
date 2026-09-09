import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const read = (path) => readFile(path, "utf8");

test("docker image uses verified release archives instead of compiling source", async () => {
  const dockerfile = await read("Dockerfile");

  assert.equal(/FROM rust:/i.test(dockerfile), false);
  assert.equal(/cargo build/i.test(dockerfile), false);
  assert.equal(/pnpm (build|install)/i.test(dockerfile), false);
  assert.equal(/go build/i.test(dockerfile), false);

  assert.match(dockerfile, /TARGETARCH/);
  assert.match(dockerfile, /releases\/latest/);
  assert.match(dockerfile, /\.assets\[\]/);
  assert.match(dockerfile, /sha256sum/);
  assert.match(dockerfile, /ai-switch-server_\$\{VERSION\}_linux-\$\{ARCH\}\.zip/);
  assert.match(dockerfile, /ai-switch-server/);
  assert.match(dockerfile, /ai-switch-tsnet/);
  assert.match(dockerfile, /web\/index\.html/);
  assert.match(dockerfile, /useradd .*ai-switch/);
  assert.match(dockerfile, /install -d .*\/home\/ai-switch\/\.ai-switch/);
});

test("docker compose runs the published multi-arch image", async () => {
  const compose = await read("deploy/docker-compose.yml");

  assert.match(compose, /image: \$\{AI_SWITCH_DOCKER_IMAGE:-[^}]+\}/);
  assert.equal(/build:/i.test(compose), false);
});

test("release workflow publishes docker hub images after the github release", async () => {
  const workflow = await read(".github/workflows/release.yml");

  const dockerJob = workflow.slice(workflow.indexOf("  publish-image:"));
  assert.notEqual(dockerJob, "");
  assert.match(dockerJob, /needs:\s*\n\s*- publish/);
  assert.match(dockerJob, /DOCKERHUB_USERNAME/);
  assert.match(dockerJob, /DOCKERHUB_TOKEN/);
  assert.match(dockerJob, /docker\/login-action@/);
  assert.match(dockerJob, /docker\/build-push-action@/);
  assert.match(dockerJob, /linux\/amd64,linux\/arm64/);
  assert.match(dockerJob, /AI_SWITCH_VERSION=\$\{\{ github\.ref_name \}\}/);
  assert.match(dockerJob, /AI_SWITCH_REPOSITORY=\$\{\{ github\.repository \}\}/);
  assert.match(dockerJob, /type=semver,pattern=\{\{version\}\}/);
});

test("standalone server fails fast when release environment configuration is invalid", async () => {
  const server = await read("src-tauri/src/server.rs");

  assert.match(
    server,
    /apply_env_config\(&pool\)\s*\.await\s*\.map_err\(\|error\| error\.to_string\(\)\)\?/,
  );
  assert.equal(/SaaS environment configuration failed/.test(server), false);
});
