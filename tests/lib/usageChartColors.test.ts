import { describe, expect, it } from "vitest";
import { assignSeriesColors, otherSeriesKey } from "../../src/lib/usageChartColors";

describe("assignSeriesColors", () => {
  it("gives every visible series its own colour", () => {
    const keys = ["a", "b", "c", "d", "e", "f", "g", "h"];

    const colors = assignSeriesColors(keys);

    expect(new Set(colors.values()).size).toBe(keys.length);
    expect(keys.every((key) => colors.get(key)?.startsWith("#"))).toBe(true);
  });

  it("keeps a series' colour when the ranking around it changes", () => {
    // Flipping the period reshuffles the order. A reader who just learned that
    // one model is blue must not find it repainted.
    const before = assignSeriesColors(["opus", "haiku", "sonnet"]);
    const after = assignSeriesColors(["sonnet", "opus", "haiku"]);

    expect(after.get("opus")).toBe(before.get("opus"));
    expect(after.get("haiku")).toBe(before.get("haiku"));
    expect(after.get("sonnet")).toBe(before.get("sonnet"));
  });

  it("paints the folded tail neutral rather than giving it a ninth hue", () => {
    const colors = assignSeriesColors(["a", "b", otherSeriesKey]);

    const other = colors.get(otherSeriesKey);
    expect(other).toBe("#a8a29e");
    expect(other).not.toBe(colors.get("a"));
    expect(other).not.toBe(colors.get("b"));
  });
});
