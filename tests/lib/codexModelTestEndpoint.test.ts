import { beforeEach, describe, expect, it } from "vitest";
import {
  CODEX_MODEL_TEST_ENDPOINT_STORAGE_KEY,
  codexModelTestInterfaceFormat,
  loadCodexModelTestEndpoint,
  saveCodexModelTestEndpoint,
} from "../../src/lib/codexModelTestEndpoint";

describe("codexModelTestEndpoint", () => {
  beforeEach(() => window.localStorage.clear());

  it("defaults to responses and maps both endpoints", () => {
    expect(loadCodexModelTestEndpoint()).toBe("/responses");
    expect(codexModelTestInterfaceFormat("/responses")).toBe("openai-responses");
    expect(codexModelTestInterfaceFormat("/chat/completions")).toBe("openai");
  });

  it("loads valid values and falls back for invalid values", () => {
    window.localStorage.setItem(
      CODEX_MODEL_TEST_ENDPOINT_STORAGE_KEY,
      "/chat/completions",
    );
    expect(loadCodexModelTestEndpoint()).toBe("/chat/completions");

    window.localStorage.setItem(
      CODEX_MODEL_TEST_ENDPOINT_STORAGE_KEY,
      "/v1/responses",
    );
    expect(loadCodexModelTestEndpoint()).toBe("/responses");
  });

  it("persists selections and tolerates unavailable storage", () => {
    saveCodexModelTestEndpoint("/chat/completions");
    expect(
      window.localStorage.getItem(CODEX_MODEL_TEST_ENDPOINT_STORAGE_KEY),
    ).toBe("/chat/completions");

    expect(() =>
      loadCodexModelTestEndpoint({
        getItem: () => {
          throw new Error("blocked");
        },
      }),
    ).not.toThrow();
    expect(() =>
      saveCodexModelTestEndpoint("/responses", {
        setItem: () => {
          throw new Error("blocked");
        },
      }),
    ).not.toThrow();
  });
});
