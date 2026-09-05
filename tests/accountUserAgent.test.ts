import { describe, expect, it } from "vitest";
import {
  BROWSER_USER_AGENT,
  CODEX_CLI_USER_AGENT,
  CLAUDE_CLI_USER_AGENT,
  GROK_WORKSPACE_USER_AGENT,
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
    expect(matchUserAgentPreset(GROK_WORKSPACE_USER_AGENT)).toBe("grok-workspace");
    expect(matchUserAgentPreset(CODEX_CLI_USER_AGENT)).toBe("codex-cli");
    expect(matchUserAgentPreset(CLAUDE_CLI_USER_AGENT)).toBe("claude-cli");
    expect(matchUserAgentPreset(BROWSER_USER_AGENT)).toBe("browser");
    expect(matchUserAgentPreset("SomethingElse/1.0")).toBe("custom");
    // Outdated CPA export (grok-cli) 被后端强制覆盖，不认作预设
    expect(matchUserAgentPreset("grok-cli")).toBe("custom");
  });

  it("keeps CLI presets in the fingerprinted shape gateways check", () => {
    expect(CODEX_CLI_USER_AGENT).toMatch(/^codex_cli_rs\/\d+\.\d+\.\d+ \(.+\) Terminal$/);
    expect(CLAUDE_CLI_USER_AGENT).toMatch(/^claude-cli\/\d+\.\d+\.\d+ \(external, cli\)$/);
  });
});
