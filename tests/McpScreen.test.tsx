import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  mcpGetMarketplaceServerDetail,
  mcpInstallFromMarketplace,
  mcpListMarketplaces,
  mcpRemoveServer,
  mcpScanLocal,
  mcpSearchMarketplace,
  mcpUpsertLocalServer,
} from "../src/lib/api/client";
import { ApiClientError } from "../src/lib/api/errors";
import { I18nProvider } from "../src/lib/i18n";
import { McpScreen } from "../src/screens/McpScreen";

vi.mock("../src/lib/api/client", () => ({
  mcpGetMarketplaceServerDetail: vi.fn(),
  mcpInstallFromMarketplace: vi.fn(),
  mcpListMarketplaces: vi.fn(),
  mcpRemoveServer: vi.fn(),
  mcpScanLocal: vi.fn(),
  mcpSearchMarketplace: vi.fn(),
  mcpUpsertLocalServer: vi.fn(),
}));

function renderScreen(language: "en" | "zh-CN") {
  return render(
    <I18nProvider initialLanguage={language}>
      <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
        <McpScreen />
      </QueryClientProvider>
    </I18nProvider>,
  );
}

describe("McpScreen", () => {
  beforeEach(() => {
    window.localStorage.clear();
    vi.clearAllMocks();
  });

  it("renders local MCP controls in Simplified Chinese", async () => {
    vi.mocked(mcpScanLocal).mockResolvedValue([]);
    vi.mocked(mcpListMarketplaces).mockResolvedValue([]);

    renderScreen("zh-CN");

    expect(await screen.findByText("MCP 服务器")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "添加服务器" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "本地配置" })).toBeInTheDocument();
  });

  it("uses the localized message for a structured MCP error", async () => {
    vi.mocked(mcpScanLocal).mockRejectedValue(
      new ApiClientError("raw backend text", "mcp.config_invalid", null, true, null),
    );
    vi.mocked(mcpListMarketplaces).mockResolvedValue([]);

    renderScreen("zh-CN");

    expect(await screen.findByRole("alert")).toHaveTextContent("MCP 配置无效");
    expect(screen.getByRole("alert")).not.toHaveTextContent("raw backend text");
  });

  it("keeps the English labels available", async () => {
    vi.mocked(mcpScanLocal).mockResolvedValue([]);
    vi.mocked(mcpListMarketplaces).mockResolvedValue([]);

    renderScreen("en");

    expect(await screen.findByText("MCP servers")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Add server" })).toBeInTheDocument();
  });
});
