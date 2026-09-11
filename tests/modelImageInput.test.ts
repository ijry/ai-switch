import { describe, expect, it } from "vitest";
import { defaultSupportsImageInput } from "../src/lib/modelImageInput";

describe("default image input support", () => {
  it("matches DeepSeek v4.1 by prefix and supports image input", () => {
    expect(defaultSupportsImageInput("deepseek-v4.1")).toBe(true);
    expect(defaultSupportsImageInput("deepseek-v4.1-preview")).toBe(true);
  });

  it("applies the requested family defaults", () => {
    expect(defaultSupportsImageInput("gpt-5.6-sol")).toBe(true);
    expect(defaultSupportsImageInput("deepseek-v4-flash-0731")).toBe(false);
    expect(defaultSupportsImageInput("glm-5.3")).toBe(false);
    expect(defaultSupportsImageInput("glm-5.2-flash")).toBe(true);
    expect(defaultSupportsImageInput("qwen3-vl-plus")).toBe(true);
    expect(defaultSupportsImageInput("qwen3-plus")).toBe(false);
    expect(defaultSupportsImageInput("unknown-model")).toBe(false);
  });

  it("normalizes provider prefixes and separators", () => {
    expect(defaultSupportsImageInput("openai/GPT_5.5")).toBe(true);
    expect(defaultSupportsImageInput("DeepSeek_V4.1")).toBe(true);
  });
});
