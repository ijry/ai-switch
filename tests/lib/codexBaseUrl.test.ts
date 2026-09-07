import { describe, expect, it } from "vitest";
import {
  adjustCodexBaseUrlForInterfaceFormat,
  type CodexBaseUrlAdjustment,
} from "../../src/lib/codexBaseUrl";

describe("adjustCodexBaseUrlForInterfaceFormat", () => {
  it.each([
    ["https://api.test/v1", "https://api.test"],
    ["https://api.test/proxy/v1", "https://api.test/proxy"],
    ["http://localhost:3000/v1/", "http://localhost:3000"],
    ["https://api.test/v1/proxy/v1", "https://api.test/v1/proxy"],
    [" https://api.test/v1 ", "https://api.test"],
    ["https://api.test/v1?key=value#section", "https://api.test?key=value#section"],
  ])("removes only the trailing v1 segment from %s for Claude", (originalBaseUrl, adjustedBaseUrl) => {
    expect(adjustCodexBaseUrlForInterfaceFormat("codex", originalBaseUrl, "anthropic")).toEqual({
      originalBaseUrl,
      adjustedBaseUrl,
      action: "remove-v1",
    });
  });

  it.each(["openai", "openai-responses"] as const)("restores the original URL for %s", (interfaceFormat) => {
    const originalBaseUrl = "https://api.test/proxy/v1/";
    const removed = adjustCodexBaseUrlForInterfaceFormat("codex", originalBaseUrl, "anthropic");

    expect(adjustCodexBaseUrlForInterfaceFormat("codex", "https://api.test/proxy", interfaceFormat, removed)).toEqual({
      originalBaseUrl: "https://api.test/proxy",
      adjustedBaseUrl: originalBaseUrl,
      action: "restore-v1",
    });
  });

  it.each([
    "",
    "/v1",
    "not-a-url/v1",
    "ftp://api.test/v1",
    "https://v1",
    "https://api.test",
    "https://api.test/proxy",
    "https://api.test/v1beta",
    "https://api.test/v10",
    "https://api.test/v1/proxy",
    "https://api.test/proxy?next=/v1",
    "https://api.test/proxy#/v1",
  ])("leaves URLs without a valid trailing v1 path unchanged: %s", (baseUrl) => {
    expect(adjustCodexBaseUrlForInterfaceFormat("codex", baseUrl, "anthropic")).toBeNull();
    expect(adjustCodexBaseUrlForInterfaceFormat("codex", baseUrl, "openai")).toBeNull();
    expect(adjustCodexBaseUrlForInterfaceFormat("codex", baseUrl, "openai-responses")).toBeNull();
  });

  it.each(["claude", "gemini", "opencode", "openclaw", "hermes", "grok"] as const)(
    "does not adjust URLs on the %s platform",
    (platform) => {
      expect(adjustCodexBaseUrlForInterfaceFormat(platform, "https://api.test/v1", "anthropic")).toBeNull();
    },
  );

  it("does not adjust a URL when switching between OpenAI formats or to Gemini", () => {
    for (const interfaceFormat of ["openai", "openai-responses", "gemini"] as const) {
      expect(adjustCodexBaseUrlForInterfaceFormat("codex", "https://api.test/v1", interfaceFormat)).toBeNull();
    }
  });

  it("does not restore a stale URL or append a duplicate v1 segment", () => {
    const removed = adjustCodexBaseUrlForInterfaceFormat("codex", "https://api.test/v1", "anthropic");

    for (const baseUrl of ["https://changed.test", "https://api.test/v1"]) {
      expect(adjustCodexBaseUrlForInterfaceFormat("codex", baseUrl, "openai", removed)).toBeNull();
    }
  });

  it("supports repeated format round trips without accumulating v1 segments", () => {
    let baseUrl = "https://api.test/proxy/v1";
    let adjustment: CodexBaseUrlAdjustment | null = null;
    for (const interfaceFormat of ["anthropic", "openai", "anthropic", "openai-responses"] as const) {
      adjustment = adjustCodexBaseUrlForInterfaceFormat("codex", baseUrl, interfaceFormat, adjustment);
      expect(adjustment).not.toBeNull();
      baseUrl = adjustment!.adjustedBaseUrl;
    }
    expect(baseUrl).toBe("https://api.test/proxy/v1");
  });
});
