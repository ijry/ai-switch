export type CodexModelTestEndpoint = "/responses" | "/chat/completions";

export const CODEX_MODEL_TEST_ENDPOINT_STORAGE_KEY =
  "ai-switch.codex-model-test-endpoint";

const DEFAULT_CODEX_MODEL_TEST_ENDPOINT: CodexModelTestEndpoint = "/responses";

function isCodexModelTestEndpoint(
  value: string | null,
): value is CodexModelTestEndpoint {
  return value === "/responses" || value === "/chat/completions";
}

export function loadCodexModelTestEndpoint(
  storage: Pick<Storage, "getItem"> = window.localStorage,
): CodexModelTestEndpoint {
  try {
    const stored = storage.getItem(CODEX_MODEL_TEST_ENDPOINT_STORAGE_KEY);
    return isCodexModelTestEndpoint(stored)
      ? stored
      : DEFAULT_CODEX_MODEL_TEST_ENDPOINT;
  } catch {
    return DEFAULT_CODEX_MODEL_TEST_ENDPOINT;
  }
}

export function saveCodexModelTestEndpoint(
  endpoint: CodexModelTestEndpoint,
  storage: Pick<Storage, "setItem"> = window.localStorage,
): void {
  try {
    storage.setItem(CODEX_MODEL_TEST_ENDPOINT_STORAGE_KEY, endpoint);
  } catch {
    // Storage can be unavailable in restricted browser contexts.
  }
}

export function codexModelTestInterfaceFormat(
  endpoint: CodexModelTestEndpoint,
): "openai-responses" | "openai" {
  return endpoint === "/responses" ? "openai-responses" : "openai";
}
