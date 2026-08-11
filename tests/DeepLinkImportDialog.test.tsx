import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  createApiRouteCredential,
  getRoutePool,
  setRoutePoolMembers,
} from "../src/lib/api/client";
import { DeepLinkImportDialog, type DeepLinkProviderImportPayload } from "../src/components/deeplink/DeepLinkImportDialog";

const transportState = vi.hoisted(() => ({
  importHandler: null as ((payload: DeepLinkProviderImportPayload) => void) | null,
  errorHandler: null as ((payload: { message: string; source: string }) => void) | null,
  subscribe: vi.fn(),
}));

vi.mock("../src/lib/api/client", () => ({
  createApiRouteCredential: vi.fn(),
  getRoutePool: vi.fn(),
  setRoutePoolMembers: vi.fn(),
}));

vi.mock("../src/lib/transport", () => ({
  getTransport: () => transportState,
  isDesktop: () => true,
}));

const payload: DeepLinkProviderImportPayload = {
  scheme: "ccswitch",
  version: "1",
  resource: "api",
  app: "ai-switch",
  platform: "codex",
  display_name: "Imported API",
  base_url: "https://api.example.com/v1",
  api_key_masked: "sk-...1234",
  api_key: "sk-secret",
  interface_format: "openai",
  model_mappings_json: "[]",
  source_url_sanitized: "ccswitch://import",
};

const created = {
  id: "cred-imported",
  platform: "codex",
};

function renderDialog(onImported = vi.fn()) {
  render(<DeepLinkImportDialog onImported={onImported} />);
  return onImported;
}

describe("DeepLinkImportDialog", () => {
  beforeEach(() => {
    transportState.importHandler = null;
    transportState.errorHandler = null;
    transportState.subscribe.mockReset();
    transportState.subscribe.mockImplementation(
      async (event: string, handler: (payload: unknown) => void) => {
        if (event === "deeplink-import") {
          transportState.importHandler = handler as (payload: DeepLinkProviderImportPayload) => void;
        } else if (event === "deeplink-error") {
          transportState.errorHandler = handler as (payload: { message: string; source: string }) => void;
        }
        return () => undefined;
      },
    );
    vi.mocked(createApiRouteCredential).mockReset();
    vi.mocked(getRoutePool).mockReset();
    vi.mocked(setRoutePoolMembers).mockReset();
    vi.mocked(createApiRouteCredential).mockResolvedValue(created as never);
    vi.mocked(getRoutePool).mockResolvedValue({
      platform: "codex",
      account_ids: ["existing-account"],
      stats: {
        member_count: 1,
        request_count: 0,
        token_count: 0,
        input_token_count: 0,
        output_token_count: 0,
        cache_token_count: 0,
        cost_micros: 0,
        recent_logs: [],
        requests: [],
        request_row_count: 0,
        request_page: 1,
        request_page_size: 20,
      },
    });
    vi.mocked(setRoutePoolMembers).mockResolvedValue({} as never);
  });

  it("joins the existing pool by default and reports the pooled segment", async () => {
    const onImported = renderDialog();
    await waitFor(() => expect(transportState.importHandler).not.toBeNull());

    act(() => {
      transportState.importHandler?.(payload);
    });
    expect(await screen.findByRole("dialog", { name: "导入 API 账号" })).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "确认导入" }));

    await waitFor(() =>
      expect(setRoutePoolMembers).toHaveBeenCalledWith({
        platform: "codex",
        account_ids: ["existing-account", "cred-imported"],
      }),
    );
    expect(onImported).toHaveBeenCalledWith("codex", { joinedPool: true });
  });

  it("skips pool membership when the import checkbox is cleared", async () => {
    const onImported = renderDialog();
    await waitFor(() => expect(transportState.importHandler).not.toBeNull());

    act(() => {
      transportState.importHandler?.(payload);
    });
    await screen.findByRole("dialog", { name: "导入 API 账号" });
    fireEvent.click(screen.getByRole("checkbox", { name: /导入后加入算力池/ }));
    await userEvent.click(screen.getByRole("button", { name: "确认导入" }));

    await waitFor(() => expect(onImported).toHaveBeenCalledWith("codex", { joinedPool: false }));
    expect(setRoutePoolMembers).not.toHaveBeenCalled();
    expect(getRoutePool).not.toHaveBeenCalled();
  });
});
