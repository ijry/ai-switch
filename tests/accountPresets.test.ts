import { describe, expect, it } from "vitest";
import {
  ACCOUNT_PRESETS,
  matchPresetByBaseUrl,
  presetsForPlatform,
} from "../src/lib/accountPresets";

describe("accountPresets", () => {
  it("exposes both AgentRouter lines for codex", () => {
    const presets = presetsForPlatform("codex");

    expect(presets).toHaveLength(2);
    expect(presets.map((preset) => preset.baseUrl)).toEqual([
      "https://agentrouter.org/v1",
      "https://ps.air-outer.com/v1",
    ]);
  });

  it("returns no presets for platforms without any", () => {
    expect(presetsForPlatform("claude")).toEqual([]);
    expect(presetsForPlatform("gemini")).toEqual([]);
    expect(presetsForPlatform("grok")).toEqual([]);
  });

  it("describes the AgentRouter primary line completely", () => {
    const preset = presetsForPlatform("codex")[0];

    expect(preset.platform).toBe("codex");
    expect(preset.label).toBe("AgentRouter (agentrouter.org)");
    expect(preset.defaultName).toBe("AgentRouter");
    expect(preset.baseUrl).toBe("https://agentrouter.org/v1");
    expect(preset.interfaceFormat).toBe("openai");
    expect(preset.modelMappings).toEqual([
      { from: "gpt-5.6-sol", to: "gpt-5.6-sol" },
    ]);
  });

  it("describes the AgentRouter backup line completely", () => {
    const preset = presetsForPlatform("codex")[1];

    expect(preset.label).toBe("AgentRouter (ps.air-outer.com)");
    expect(preset.defaultName).toBe("AgentRouter 备用");
    expect(preset.baseUrl).toBe("https://ps.air-outer.com/v1");
    expect(preset.interfaceFormat).toBe("openai");
    expect(preset.modelMappings).toEqual([
      { from: "gpt-5.6-sol", to: "gpt-5.6-sol" },
    ]);
  });

  it("keeps every preset id and default name unique", () => {
    const ids = ACCOUNT_PRESETS.map((preset) => preset.id);
    const names = ACCOUNT_PRESETS.map((preset) => preset.defaultName);

    expect(new Set(ids).size).toBe(ids.length);
    expect(new Set(names).size).toBe(names.length);
  });

  it("matches a base url regardless of case, spacing or trailing slash", () => {
    for (const value of [
      "https://agentrouter.org/v1",
      "https://agentrouter.org/v1/",
      "https://AgentRouter.org/v1",
      "  https://agentrouter.org/v1  ",
    ]) {
      expect(matchPresetByBaseUrl("codex", value)?.id).toBe(
        presetsForPlatform("codex")[0].id,
      );
    }
  });

  it("matches the backup line independently", () => {
    expect(matchPresetByBaseUrl("codex", "https://ps.air-outer.com/v1")?.id).toBe(
      presetsForPlatform("codex")[1].id,
    );
  });

  it("returns null for unknown, empty or near-miss base urls", () => {
    expect(matchPresetByBaseUrl("codex", "https://api.example.com/v1")).toBeNull();
    expect(matchPresetByBaseUrl("codex", "")).toBeNull();
    expect(matchPresetByBaseUrl("codex", "   ")).toBeNull();
    // Different endpoints must not be mistaken for the primary line.
    expect(matchPresetByBaseUrl("codex", "https://agentrouter.org/v2")).toBeNull();
    expect(matchPresetByBaseUrl("codex", "https://agentrouter.org")).toBeNull();
  });

  it("scopes matching to the requested platform", () => {
    expect(matchPresetByBaseUrl("claude", "https://agentrouter.org/v1")).toBeNull();
  });
});
