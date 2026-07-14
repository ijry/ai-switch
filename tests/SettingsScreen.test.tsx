import { QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  disconnectTailscale,
  getSettings,
  getTailscaleStatus,
  getWebServerStatus,
  getWebServiceConfig,
  saveSettings,
  saveWebServiceConfig,
  startTailscaleLogin,
  startTailscaleWithAuthKey,
  startWebServer,
  stopWebServer,
} from "../src/lib/api/client";
import { I18nProvider } from "../src/lib/i18n";
import { createQueryClient } from "../src/lib/query/queryClient";
import { SettingsScreen } from "../src/screens/SettingsScreen";
import { settingsFixture } from "../src/test/fixtures";

vi.mock("../src/lib/api/client", () => ({
  getSettings: vi.fn(),
  saveSettings: vi.fn(),
  getWebServiceConfig: vi.fn(),
  saveWebServiceConfig: vi.fn(),
  getWebServerStatus: vi.fn(),
  startWebServer: vi.fn(),
  stopWebServer: vi.fn(),
  getTailscaleStatus: vi.fn(),
  startTailscaleLogin: vi.fn(),
  startTailscaleWithAuthKey: vi.fn(),
  disconnectTailscale: vi.fn(),
}));

describe("SettingsScreen", () => {
  beforeEach(() => {
    window.localStorage.clear();
    vi.mocked(getSettings).mockReset();
    vi.mocked(saveSettings).mockReset();
    vi.mocked(getWebServiceConfig).mockReset();
    vi.mocked(saveWebServiceConfig).mockReset();
    vi.mocked(getWebServerStatus).mockReset();
    vi.mocked(startWebServer).mockReset();
    vi.mocked(stopWebServer).mockReset();
    vi.mocked(getTailscaleStatus).mockReset();
    vi.mocked(startTailscaleLogin).mockReset();
    vi.mocked(startTailscaleWithAuthKey).mockReset();
    vi.mocked(disconnectTailscale).mockReset();
    vi.mocked(getWebServiceConfig).mockResolvedValue({
      host: "127.0.0.1",
      port: 3090,
      token: "secret",
      autoStart: false,
      tailscaleEnabled: true,
    });
    vi.mocked(getWebServerStatus).mockResolvedValue({
      running: false,
      host: "127.0.0.1",
      port: null,
      baseUrl: null,
    });
    vi.mocked(saveWebServiceConfig).mockImplementation(async (config) => config);
    vi.mocked(startWebServer).mockResolvedValue({
      running: true,
      host: "127.0.0.1",
      port: 3090,
      baseUrl: "http://127.0.0.1:3090",
    });
    vi.mocked(stopWebServer).mockResolvedValue({
      running: false,
      host: "127.0.0.1",
      port: null,
      baseUrl: null,
    });
    vi.mocked(getTailscaleStatus).mockResolvedValue({
      state: "notConnected",
      deviceName: null,
      tailnetIp: null,
      message: null,
    });
    vi.mocked(startTailscaleLogin).mockResolvedValue({
      loginUrl: null,
      message: "login started",
    });
    vi.mocked(startTailscaleWithAuthKey).mockResolvedValue({
      state: "connected",
      deviceName: "ai-switch",
      tailnetIp: "100.64.0.12",
      accessUrls: ["http://100.64.0.12:3090"],
      serving: true,
      message: null,
    });
    vi.mocked(disconnectTailscale).mockResolvedValue({
      state: "notConnected",
      deviceName: null,
      tailnetIp: null,
      message: null,
    });
  });

  it("loads settings and saves a toggled theme value", async () => {
    vi.mocked(getSettings).mockResolvedValue(settingsFixture);
    vi.mocked(saveSettings).mockImplementation(async (settings) => settings);

    render(
      <QueryClientProvider client={createQueryClient()}>
        <I18nProvider initialLanguage="zh-CN">
          <SettingsScreen />
        </I18nProvider>
      </QueryClientProvider>,
    );

    expect(await screen.findByText(`数据目录：${settingsFixture.data_dir}`)).toBeInTheDocument();
    expect(screen.getByText("功能入口")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /会话/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /更新/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /日志/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Web 服务/ })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /MCP/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /批量/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /实例/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /唤醒任务/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /AI 模型/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /导入/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /目标/ })).not.toBeInTheDocument();
    expect(await screen.findByRole("heading", { name: "Web 服务" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /使用 OAuth 登录/ })).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "切换主题值" }));

    await waitFor(() => expect(saveSettings).toHaveBeenCalled());
    expect(vi.mocked(saveSettings).mock.calls[0][0]).toEqual({
      ...settingsFixture,
      language: "zh-CN",
      theme: "dark",
    });
    expect(await screen.findByText("设置已保存。")).toBeInTheDocument();
  });

  it("saves language changes and updates the selector", async () => {
    const englishSettings = { ...settingsFixture, language: "en" };
    vi.mocked(getSettings).mockResolvedValue(englishSettings);
    vi.mocked(saveSettings).mockImplementation(async (settings) => settings);

    render(
      <QueryClientProvider client={createQueryClient()}>
        <I18nProvider initialLanguage="en">
          <SettingsScreen />
        </I18nProvider>
      </QueryClientProvider>,
    );

    const selector = await screen.findByRole("combobox", { name: "Language" });
    await userEvent.selectOptions(selector, "zh-CN");

    await waitFor(() =>
      expect(vi.mocked(saveSettings).mock.calls[0][0]).toEqual({
        ...englishSettings,
        language: "zh-CN",
      }),
    );
    expect(selector).toHaveValue("zh-CN");
  });

  it("opens feature entries through the settings hub", async () => {
    const onOpenFeature = vi.fn();
    vi.mocked(getSettings).mockResolvedValue(settingsFixture);

    render(
      <QueryClientProvider client={createQueryClient()}>
        <I18nProvider initialLanguage="zh-CN">
          <SettingsScreen onOpenFeature={onOpenFeature} />
        </I18nProvider>
      </QueryClientProvider>,
    );

    await userEvent.click(await screen.findByRole("button", { name: /会话/ }));
    expect(onOpenFeature).toHaveBeenCalledWith("Sessions");
  });
});

