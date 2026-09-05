import { afterEach, describe, expect, it, vi } from "vitest";
import { copyPlainText } from "../../src/lib/copyToClipboard";

const originalExecCommand = document.execCommand;

function setClipboard(value: unknown) {
  Object.defineProperty(navigator, "clipboard", { configurable: true, value });
}

afterEach(() => {
  document.execCommand = originalExecCommand;
  setClipboard(undefined);
});

describe("copyPlainText", () => {
  it("uses the async clipboard when the origin is a secure context", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    setClipboard({ writeText });

    await expect(copyPlainText("share me")).resolves.toBe(true);
    expect(writeText).toHaveBeenCalledWith("share me");
  });

  it("falls back to a hidden textarea when the clipboard api is missing", async () => {
    let stagedText: string | null = null;
    document.execCommand = vi.fn(() => {
      stagedText = document.querySelector("textarea")?.value ?? null;
      return true;
    });

    await expect(copyPlainText("share me")).resolves.toBe(true);
    expect(document.execCommand).toHaveBeenCalledWith("copy");
    expect(stagedText).toBe("share me");
    // The scratch textarea must not outlive the copy.
    expect(document.querySelector("textarea")).toBeNull();
  });

  it("falls back to the textarea when the clipboard api rejects", async () => {
    setClipboard({ writeText: vi.fn().mockRejectedValue(new Error("denied")) });
    document.execCommand = vi.fn(() => true);

    await expect(copyPlainText("share me")).resolves.toBe(true);
    expect(document.execCommand).toHaveBeenCalledWith("copy");
  });

  it("reports failure when neither path can copy", async () => {
    document.execCommand = vi.fn(() => false);

    await expect(copyPlainText("share me")).resolves.toBe(false);
    expect(document.querySelector("textarea")).toBeNull();
  });
});
