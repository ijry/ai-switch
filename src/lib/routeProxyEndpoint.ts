/**
 * The pool address a client of `platform` has to be pointed at.
 *
 * Codex speaks the OpenAI wire format and calls `{baseUrl}/responses`, so the
 * `/v1` has to sit inside the base URL itself. The config writer already does
 * this when it renders `~/.codex/config.toml`
 * (`src-tauri/src/adapters/route_config/codex.rs`), and an address the user
 * copies by hand has to match what gets written. Claude / Gemini / Grok take the
 * bare address and append their own paths.
 */
export function routeProxyEndpointForPlatform(baseUrl: string, platform: string): string {
  const trimmed = baseUrl.trim().replace(/\/+$/, "");
  if (!trimmed || platform !== "codex") {
    return trimmed;
  }
  // Idempotent: a base URL that already ends in /v1 must not grow a second one.
  return /\/v1$/i.test(trimmed) ? trimmed : `${trimmed}/v1`;
}
