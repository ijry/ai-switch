import { describe, expect, it } from "vitest";
import { formatByteSize } from "../../src/lib/byteSize";

describe("formatByteSize", () => {
  it("keeps raw byte counts whole", () => {
    expect(formatByteSize(0)).toBe("0 B");
    expect(formatByteSize(512)).toBe("512 B");
    expect(formatByteSize(1023)).toBe("1023 B");
  });

  it("drops the decimal on exact values", () => {
    expect(formatByteSize(1024)).toBe("1 KB");
    expect(formatByteSize(640 * 1024 * 1024)).toBe("640 MB");
    expect(formatByteSize(1024 * 1024 * 1024)).toBe("1 GB");
    expect(formatByteSize(2 * 1024 ** 4)).toBe("2 TB");
  });

  it("keeps one decimal for anything in between", () => {
    expect(formatByteSize(1536)).toBe("1.5 KB");
    expect(formatByteSize(Math.round(12.34 * 1024 ** 3))).toBe("12.3 GB");
  });

  it("carries into the next unit rather than reading 1024", () => {
    // 1023.97 MB: rounds to 1024.0 in MB, which has to become 1 GB.
    expect(formatByteSize(1024 * 1024 * 1024 - 32 * 1024)).toBe("1 GB");
  });

  it("stops at terabytes instead of inventing a unit", () => {
    expect(formatByteSize(4096 * 1024 ** 4)).toBe("4096 TB");
  });

  it("treats missing or nonsense figures as zero", () => {
    expect(formatByteSize(-1)).toBe("0 B");
    expect(formatByteSize(Number.NaN)).toBe("0 B");
    expect(formatByteSize(Number.POSITIVE_INFINITY)).toBe("0 B");
  });
});
