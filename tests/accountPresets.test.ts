import { describe, expect, it } from "vitest";
import {
  ACCOUNT_PRESETS,
  matchPresetByBaseUrl,
  presetsForPlatform,
} from "../src/lib/accountPresets";
import { CLAUDE_ROLES } from "../src/lib/claude-roles";

describe("accountPresets", () => {
  it("exposes every codex line in order", () => {
    const presets = presetsForPlatform("codex");

    expect(presets).toHaveLength(3);
    expect(presets.map((preset) => preset.baseUrl)).toEqual([
      "https://agentrouter.org/v1",
      "https://ps.air-outer.com/v1",
      "https://kktoken.cc/v1",
    ]);
  });

  it("returns no presets for platforms without any", () => {
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
      { from: "glm-5.3", to: "glm-5.3" },
      { from: "deepseek-v4-flash", to: "deepseek-v4-flash" },
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
      { from: "glm-5.3", to: "glm-5.3" },
      { from: "deepseek-v4-flash", to: "deepseek-v4-flash" },
    ]);
  });

  it("passes every codex preset model through unchanged", () => {
    // A rewritten upstream name would be a silent misroute: these lines serve
    // the models under their own ids.
    for (const preset of presetsForPlatform("codex")) {
      for (const mapping of preset.modelMappings) {
        expect(mapping.to).toBe(mapping.from);
      }
    }
  });

  it("describes the KKToken line completely", () => {
    const preset = presetsForPlatform("codex")[2];

    expect(preset.id).toBe("kktoken");
    expect(preset.label).toBe("KKToken (kktoken.cc)");
    expect(preset.provider).toBe("KKToken");
    expect(preset.defaultName).toBe("KKToken");
    expect(preset.baseUrl).toBe("https://kktoken.cc/v1");
    expect(preset.interfaceFormat).toBe("openai");
    expect(preset.modelMappings).toEqual([
      { from: "claude-opus-5", to: "claude-opus-5" },
    ]);
  });

  it("describes the AgentRouter Claude line completely", () => {
    const presets = presetsForPlatform("claude");
    expect(presets).toHaveLength(2);
    const preset = presets[0];

    expect(preset.label).toBe("AgentRouter (ps.air-outer.com)");
    expect(preset.defaultName).toBe("AgentRouter Claude");
    // No /v1: the proxy appends the Anthropic path itself, and a versioned base
    // would make build_target_url strip the incoming /v1 as a duplicate.
    expect(preset.baseUrl).toBe("https://ps.air-outer.com");
    expect(preset.interfaceFormat).toBe("anthropic");
    expect(preset.modelMappings).toEqual([
      { from: "claude-sonnet-alias", to: "claude-opus-5" },
      { from: "claude-opus-alias", to: "claude-opus-5" },
      { from: "claude-fable-alias", to: "claude-opus-5" },
      { from: "claude-haiku-alias", to: "claude-opus-5" },
      { from: "claude-subagent", to: "claude-opus-5" },
      { from: "claude-model", to: "claude-opus-5" },
    ]);
  });

  it("maps every Claude role, so a role added later cannot stay unmapped", () => {
    for (const preset of presetsForPlatform("claude")) {
      const mapped = preset.modelMappings.map((mapping) => mapping.from);
      expect(mapped).toEqual(CLAUDE_ROLES.map((role) => role.alias));
    }
  });

  it("describes the GoRouter Claude line completely", () => {
    const preset = presetsForPlatform("claude")[1];

    expect(preset.id).toBe("gorouter-claude");
    expect(preset.label).toBe("GoRouter (gorouter.app)");
    expect(preset.provider).toBe("GoRouter");
    expect(preset.defaultName).toBe("GoRouter");
    // No /v1: the proxy appends the Anthropic path, and a versioned base would
    // make build_target_url strip the incoming /v1 as a duplicate.
    expect(preset.baseUrl).toBe("https://gorouter.app");
    expect(preset.interfaceFormat).toBe("anthropic");
    expect(preset.modelMappings.every((mapping) => mapping.to === "claude-opus-5")).toBe(
      true,
    );
  });

  it("leaves 1M unticked in the Claude preset", () => {
    // The proxy only sends the context-1m beta marker when a mapping declares it,
    // and an upstream without the tier answers 503 rather than ignoring it — so
    // ticking it for the user would break requests they never opted into.
    for (const preset of presetsForPlatform("claude")) {
      for (const mapping of preset.modelMappings) {
        expect(mapping.supports_1m).toBeUndefined();
      }
    }
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

  it("matches the KKToken line independently", () => {
    expect(matchPresetByBaseUrl("codex", "https://kktoken.cc/v1/")?.id).toBe("kktoken");
    expect(matchPresetByBaseUrl("codex", "https://kktoken.cc")).toBeNull();
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

  it("matches the GoRouter line", () => {
    expect(matchPresetByBaseUrl("claude", "https://gorouter.app")?.id).toBe(
      "gorouter-claude",
    );
    expect(matchPresetByBaseUrl("claude", "https://gorouter.app/")?.id).toBe(
      "gorouter-claude",
    );
    // A versioned base is a different endpoint and must not match.
    expect(matchPresetByBaseUrl("claude", "https://gorouter.app/v1")).toBeNull();
  });

  it("keeps the two ps.air-outer.com lines apart by platform and base url", () => {
    // Same host, different platform, different base: the codex line is versioned
    // and the Claude line is not, so neither can match the other's url.
    expect(matchPresetByBaseUrl("codex", "https://ps.air-outer.com/v1")?.id).toBe(
      "agentrouter-backup",
    );
    expect(matchPresetByBaseUrl("claude", "https://ps.air-outer.com")?.id).toBe(
      "agentrouter-claude",
    );
    expect(matchPresetByBaseUrl("claude", "https://ps.air-outer.com/v1")).toBeNull();
    expect(matchPresetByBaseUrl("codex", "https://ps.air-outer.com")).toBeNull();
  });
});
