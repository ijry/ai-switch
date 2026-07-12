import { invoke } from "@tauri-apps/api/core";
import { describe, expect, it, vi } from "vitest";
import {
  listProviders,
  listTargetSwitchStatuses,
  switchTargetProvider,
} from "../src/lib/api/client";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

describe("api client provider switching", () => {
  it("invokes provider and target switching commands", async () => {
    vi.mocked(invoke).mockResolvedValueOnce([]);
    await listProviders();
    expect(invoke).toHaveBeenLastCalledWith("list_providers");

    vi.mocked(invoke).mockResolvedValueOnce([]);
    await listTargetSwitchStatuses();
    expect(invoke).toHaveBeenLastCalledWith("list_target_switch_statuses");

    vi.mocked(invoke).mockResolvedValueOnce({ status: "written" });
    await switchTargetProvider({
      target_app_id: "target-1",
      provider_id: "provider-1",
      mode: "sandbox",
    });
    expect(invoke).toHaveBeenLastCalledWith("switch_target_provider", {
      request: {
        target_app_id: "target-1",
        provider_id: "provider-1",
        mode: "sandbox",
      },
    });

    vi.mocked(invoke).mockResolvedValueOnce({ status: "written" });
    await switchTargetProvider({
      target_app_id: "target-codex",
      provider_id: "provider-1",
      mode: "real",
    });
    expect(invoke).toHaveBeenLastCalledWith("switch_target_provider", {
      request: {
        target_app_id: "target-codex",
        provider_id: "provider-1",
        mode: "real",
      },
    });
  });
});
