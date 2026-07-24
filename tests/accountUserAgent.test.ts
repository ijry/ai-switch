import { describe, expect, it } from "vitest";
import {
  BROWSER_USER_AGENT,
  matchUserAgentPreset,
  readUserAgentFromConfig,
  writeUserAgentToConfig,
} from "../src/lib/accountUserAgent";

describe("accountUserAgent", () => {
  it("reads User-Agent case-insensitively", () => {
    expect(readUserAgentFromConfig({ headers: { "user-agent": "Bot/1" } })).toBe("Bot/1");
    expect(readUserAgentFromConfig({ headers: { "User-Agent": "Bot/2" } })).toBe("Bot/2");
    expect(readUserAgentFromConfig({})).toBe("");
  });

  it("writes and clears User-Agent while preserving other headers", () => {
    const withUa = writeUserAgentToConfig(
      { headers: { "X-Test": "1" }, base_url: "https://example.com" },
      "  Bot/9  ",
    );
    expect(withUa).toEqual({
      headers: { "X-Test": "1", "User-Agent": "Bot/9" },
      base_url: "https://example.com",
    });

    const cleared = writeUserAgentToConfig(withUa, "   ");
    expect(cleared).toEqual({
      headers: { "X-Test": "1" },
      base_url: "https://example.com",
    });
  });

  it("matches presets and falls back to custom", () => {
    expect(matchUserAgentPreset("")).toBe("default");
    expect(matchUserAgentPreset("xai-grok-workspace/0.2.93")).toBe("grok-workspace");
    expect(matchUserAgentPreset("grok-cli")).toBe("grok-cli");
    expect(matchUserAgentPreset(BROWSER_USER_AGENT)).toBe("browser");
    expect(matchUserAgentPreset("SomethingElse/1.0")).toBe("custom");
  });
});
