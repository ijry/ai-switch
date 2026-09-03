import { describe, expect, it } from "vitest";
import {
  codexBaselineReasoningLevels,
  codexContextWindowLabel,
  codexDefaultContextWindow,
  codexEffectiveReasoningLevels,
  normalizeCodexContextWindow,
  normalizeCodexReasoningLevels,
  usesCodexBaselineReasoning,
  CODEX_CONTEXT_WINDOW_OPTIONS,
  CODEX_REASONING_LEVEL_OPTIONS,
} from "../src/lib/codexModelCapability";

describe("codexModelCapability", () => {
  it("gives each GPT baseline model its own effort ladder", () => {
    expect(codexBaselineReasoningLevels("gpt-5.6-sol")).toEqual([
      "low",
      "medium",
      "high",
      "xhigh",
      "max",
      "ultra",
    ]);
    expect(codexBaselineReasoningLevels("gpt-5.6-luna")).toEqual([
      "low",
      "medium",
      "high",
      "xhigh",
      "max",
    ]);
    expect(codexBaselineReasoningLevels("gpt-5.5")).toEqual(["low", "medium", "high", "xhigh"]);
  });

  it("falls back to low/medium/high for a relay's own model id", () => {
    expect(codexBaselineReasoningLevels("deepseek-v4-flash")).toEqual(["low", "medium", "high"]);
    // Matching is case-insensitive so a pasted id still finds its profile.
    expect(codexBaselineReasoningLevels(" GPT-5.5 ")).toEqual(["low", "medium", "high", "xhigh"]);
  });

  it("treats an absent or empty list as following the baseline", () => {
    expect(codexEffectiveReasoningLevels("gpt-5.5", null)).toEqual([
      "low",
      "medium",
      "high",
      "xhigh",
    ]);
    expect(codexEffectiveReasoningLevels("gpt-5.5", [])).toEqual([
      "low",
      "medium",
      "high",
      "xhigh",
    ]);
    expect(usesCodexBaselineReasoning([])).toBe(true);
    expect(usesCodexBaselineReasoning(["high"])).toBe(false);
  });

  it("normalizes a declared list the same way the backend does", () => {
    expect(normalizeCodexReasoningLevels([" High ", "high", "", "ULTRA"])).toEqual([
      "high",
      "ultra",
    ]);
    expect(normalizeCodexReasoningLevels(null)).toEqual([]);
    // A hand-edited config can hold anything; only strings survive.
    expect(normalizeCodexReasoningLevels([1, "low"] as unknown as string[])).toEqual(["low"]);
  });

  it("keeps a declared list ahead of the baseline", () => {
    expect(codexEffectiveReasoningLevels("gpt-5.6-sol", ["medium", "max"])).toEqual([
      "medium",
      "max",
    ]);
  });

  it("labels both offered and imported context windows", () => {
    expect(codexContextWindowLabel(200_000)).toBe("200K");
    expect(codexContextWindowLabel(1_000_000)).toBe("1M");
    // An import can carry a size the option list does not offer.
    expect(codexContextWindowLabel(272_000)).toBe("272K");
    expect(codexContextWindowLabel(1_048_576)).toBe("1048576");
  });

  it("rejects context windows that are not a usable size", () => {
    expect(normalizeCodexContextWindow(400_000)).toBe(400_000);
    expect(normalizeCodexContextWindow("256000")).toBe(256_000);
    expect(normalizeCodexContextWindow(0)).toBeNull();
    expect(normalizeCodexContextWindow(-1)).toBeNull();
    expect(normalizeCodexContextWindow(null)).toBeNull();
    expect(normalizeCodexContextWindow("large")).toBeNull();
  });

  it("keeps the 1M option away from the Claude 1M marker", () => {
    // 1_048_576 is how the CPA format states a Claude 1M tier. A Codex row that
    // reused it would come back from a round trip looking like one.
    const values: number[] = CODEX_CONTEXT_WINDOW_OPTIONS.map((option) => option.value);
    expect(values).not.toContain(1_048_576);
    expect(values).toEqual([128_000, 200_000, 256_000, 400_000, 1_000_000]);
  });

  it("offers every effort the baseline profiles can produce", () => {
    for (const model of ["gpt-5.6-sol", "gpt-5.6-terra", "gpt-5.6-luna", "gpt-5.5", "other"]) {
      for (const level of codexBaselineReasoningLevels(model)) {
        expect(CODEX_REASONING_LEVEL_OPTIONS).toContain(level);
      }
    }
  });

  it("defaults the known 1M upstream families to 1M", () => {
    for (const upstream of [
      "deepseek-v4",
      "deepseek-v4-flash-0731",
      "glm-5.2",
      "glm-5.3-air",
      "qwen-3.8-plus",
      "kimi-k3-turbo",
      // Relays that namespace by vendor must resolve the same way.
      "z-ai/glm-5.3",
      "moonshotai/kimi-k3",
      // Case is the relay's choice, not a different model.
      "DeepSeek-V4-Flash",
      "  glm-5.3  ",
    ]) {
      expect(codexDefaultContextWindow(upstream)).toBe(1_000_000);
    }
  });

  it("defaults every other upstream model to the conservative 128K", () => {
    for (const upstream of [
      "gpt-5.6-sol",
      // An older generation of the same family is not in the table.
      "deepseek-v3-chat",
      "glm-5.1",
      "qwen-3.7",
      "kimi-k2",
      "openai/gpt-5.5",
      "",
    ]) {
      expect(codexDefaultContextWindow(upstream)).toBe(128_000);
    }
  });

  it("keeps the default table in step with the option labels", () => {
    // The editor renders the default through codexContextWindowLabel, so a value
    // the labeller cannot name would show up as raw digits.
    expect(codexContextWindowLabel(codexDefaultContextWindow("gpt-5.5"))).toBe("128K");
    expect(codexContextWindowLabel(codexDefaultContextWindow("glm-5.3"))).toBe("1M");
  });
});
