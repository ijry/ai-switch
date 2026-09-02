import { describe, expect, it } from "vitest";
import { formatCompactCount, formatExactCount } from "../../src/lib/usageFormat";

describe("formatCompactCount", () => {
  it("leaves values under 10,000 as thousands-separated digits", () => {
    expect(formatCompactCount(0)).toBe("0");
    expect(formatCompactCount(999)).toBe("999");
    expect(formatCompactCount(9_999)).toBe("9,999");
  });

  it("switches to 万 at 10,000 with one decimal", () => {
    expect(formatCompactCount(10_000)).toBe("1.0万");
    expect(formatCompactCount(25_000)).toBe("2.5万");
    expect(formatCompactCount(250_000)).toBe("25.0万");
  });

  it("switches to 百万 at 1,000,000 with two decimals", () => {
    expect(formatCompactCount(1_000_000)).toBe("1.00百万");
    expect(formatCompactCount(2_500_000)).toBe("2.50百万");
  });

  it("switches to 亿 at 100,000,000 with two decimals", () => {
    expect(formatCompactCount(100_000_000)).toBe("1.00亿");
    expect(formatCompactCount(5_584_802_591)).toBe("55.85亿");
  });

  it("picks the tier from the raw value, not from the rounded figure", () => {
    // 999,999 is below the 百万 threshold, so it stays in 万 even though the
    // rounded mantissa reads 100.0. Same one tier up: 99,999,999 renders as
    // 100.00百万 rather than jumping to 1.00亿. Both look odd and both are
    // deliberate — the tier is chosen before any rounding happens.
    expect(formatCompactCount(999_999)).toBe("100.0万");
    expect(formatCompactCount(99_999_999)).toBe("100.00百万");
  });

  it("handles negatives and non-finite input without producing garbage", () => {
    // Token counts should never be negative, but a malformed payload must not
    // render as "NaN万" in a summary card.
    expect(formatCompactCount(-1)).toBe("0");
    expect(formatCompactCount(Number.NaN)).toBe("0");
    expect(formatCompactCount(Number.POSITIVE_INFINITY)).toBe("0");
  });
});

describe("formatExactCount", () => {
  it("renders the precise figure for the tooltip", () => {
    expect(formatExactCount(5_584_802_591)).toBe("5,584,802,591");
    expect(formatExactCount(0)).toBe("0");
  });

  it("clamps invalid input the same way the compact form does", () => {
    expect(formatExactCount(-5)).toBe("0");
    expect(formatExactCount(Number.NaN)).toBe("0");
  });
});
