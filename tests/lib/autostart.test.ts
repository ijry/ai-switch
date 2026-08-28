import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  disable,
  enable,
  isEnabled,
} from "@tauri-apps/plugin-autostart";
import {
  disableAutostart,
  enableAutostart,
  isAutostartEnabled,
} from "../../src/lib/autostart";

vi.mock("@tauri-apps/plugin-autostart", () => ({
  disable: vi.fn(),
  enable: vi.fn(),
  isEnabled: vi.fn(),
}));

describe("autostart adapter", () => {
  beforeEach(() => {
    vi.mocked(disable).mockReset();
    vi.mocked(enable).mockReset();
    vi.mocked(isEnabled).mockReset();
  });

  it("returns the system registration state", async () => {
    vi.mocked(isEnabled).mockResolvedValue(true);

    await expect(isAutostartEnabled()).resolves.toBe(true);
    expect(isEnabled).toHaveBeenCalledTimes(1);
  });

  it("delegates enable and disable operations", async () => {
    await enableAutostart();
    await disableAutostart();

    expect(enable).toHaveBeenCalledTimes(1);
    expect(disable).toHaveBeenCalledTimes(1);
  });
});
