import { beforeEach, describe, expect, it, vi } from "vitest";
import { openUrl } from "@tauri-apps/plugin-opener";
import { isDesktop } from "../../src/lib/transport";
import { openExternal } from "../../src/lib/openExternal";

vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: vi.fn(),
}));

vi.mock("../../src/lib/transport", () => ({
  isDesktop: vi.fn(),
}));

describe("openExternal", () => {
  beforeEach(() => {
    vi.mocked(openUrl).mockReset();
    vi.mocked(isDesktop).mockReset();
  });

  it("hands the url to the system browser on desktop", async () => {
    vi.mocked(isDesktop).mockReturnValue(true);
    vi.mocked(openUrl).mockResolvedValue(undefined);
    const windowOpen = vi.spyOn(window, "open").mockReturnValue(null);

    await openExternal("https://example.com/docs");

    expect(openUrl).toHaveBeenCalledWith("https://example.com/docs");
    expect(windowOpen).not.toHaveBeenCalled();
    windowOpen.mockRestore();
  });

  it("falls back to a new browser tab outside the desktop runtime", async () => {
    vi.mocked(isDesktop).mockReturnValue(false);
    const windowOpen = vi.spyOn(window, "open").mockReturnValue(null);

    await openExternal("https://example.com/docs");

    expect(openUrl).not.toHaveBeenCalled();
    expect(windowOpen).toHaveBeenCalledWith(
      "https://example.com/docs",
      "_blank",
      "noopener,noreferrer",
    );
    windowOpen.mockRestore();
  });

  it("propagates desktop failures so callers can surface them", async () => {
    vi.mocked(isDesktop).mockReturnValue(true);
    vi.mocked(openUrl).mockRejectedValue(new Error("forbidden url"));

    await expect(openExternal("https://example.com/docs")).rejects.toThrow("forbidden url");
  });
});
