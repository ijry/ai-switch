/**
 * The pool address a client of `platform` has to be pointed at.
 *
 * Two shapes, decided by whether the client appends a bare endpoint or a
 * versioned path of its own:
 *
 * - **`{base}/v1`** — Codex calls `{baseUrl}/responses`, and OpenCode / OpenClaw
 *   / Hermes all reach the pool through Chat Completions, i.e.
 *   `{baseUrl}/chat/completions`. Each platform's config writer renders exactly
 *   this, and an address the user copies by hand has to match what gets written.
 * - **bare** — Claude / Gemini / Grok clients append their own versioned paths
 *   (`/v1/messages`, `/v1beta/models/...`), so a `/v1` here would double it.
 */
const PLATFORMS_NEEDING_V1 = new Set(["codex", "opencode", "openclaw", "hermes"]);

export function routeProxyEndpointForPlatform(baseUrl: string, platform: string): string {
  const trimmed = baseUrl.trim().replace(/\/+$/, "");
  if (!trimmed || !PLATFORMS_NEEDING_V1.has(platform)) {
    return trimmed;
  }
  // Idempotent: a base URL that already ends in /v1 must not grow a second one.
  return /\/v1$/i.test(trimmed) ? trimmed : `${trimmed}/v1`;
}
