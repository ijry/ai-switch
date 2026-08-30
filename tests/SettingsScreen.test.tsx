import { QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  createMobilePairing,
  disableRouteProxyHttps,
  deleteRouteProxyHttpsCertificates,
  disconnectTailscale,
  enableRouteProxyHttps,
  getRouteProxyHttpsStatus,
  getRouteProxyStatus,
  getSettings,
  getTailscaleStatus,
  getWebServerStatus,
  getWebServiceConfig,
  openRouteProxyHttpsCertificateDirectory,
  regenerateRouteProxyHttpsCertificates,
  reimportRouteProxyRootCa,
  saveSettings,
  saveWebServiceConfig,
  startTailscaleLogin,
  startTailscaleWithAuthKey,
  startWebServer,
  stopWebServer,
  uninstallRouteProxyRootCa,
} from "../src/lib/api/client";
import { I18nProvider } from "../src/lib/i18n";
import { createQueryClient } from "../src/lib/query/queryClient";
import { SettingsScreen } from "../src/screens/SettingsScreen";
import { settingsFixture } from "../src/test/fixtures";

vi.mock("../src/lib/api/client", () => ({
  createMobilePairing: vi.fn(),
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
  getRouteProxyStatus: vi.fn(),
  getRouteProxyHttpsStatus: vi.fn(),
  enableRouteProxyHttps: vi.fn(),
  disableRouteProxyHttps: vi.fn(),
  reimportRouteProxyRootCa: vi.fn(),
  regenerateRouteProxyHttpsCertificates: vi.fn(),
  uninstallRouteProxyRootCa: vi.fn(),
  deleteRouteProxyHttpsCertificates: vi.fn(),
  openRouteProxyHttpsCertificateDirectory: vi.fn(),
}));

const httpsStatusFixture = {
  enabled: false,
  certReady: false,
  trustStatus: "untrusted" as const,
  trustAdapter: null,
  rootFingerprint: null,
  expiresAt: null,
  certificateDir: "C:/Users/example/.ai-switch/certs/route-proxy",
  rootCertificatePath: null,
  proxyBaseUrl: null,
  message: null,
  manualInstructions: [],
};

const httpsOutcomeFixture = {
  https: {
    ...httpsStatusFixture,
    enabled: true,
    certReady: true,
    trustStatus: "systemTrusted" as const,
    rootFingerprint: "a".repeat(64),
    expiresAt: "2027-07-25T00:00:00Z",
    proxyBaseUrl: "https://127.0.0.1:8317",
  },
  routeProxy: {
    running: true,
    bind_host: "127.0.0.1",
    port: 8317,
    base_url: "https://127.0.0.1:8317",
  },
  configWrites: [],
};

describe("SettingsScreen", () => {
  beforeEach(() => {
    window.localStorage.clear();
    vi.mocked(createMobilePairing).mockReset();
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
    vi.mocked(getRouteProxyStatus).mockReset();
    vi.mocked(getRouteProxyHttpsStatus).mockReset();
    vi.mocked(enableRouteProxyHttps).mockReset();
    vi.mocked(disableRouteProxyHttps).mockReset();
    vi.mocked(reimportRouteProxyRootCa).mockReset();
    vi.mocked(regenerateRouteProxyHttpsCertificates).mockReset();
    vi.mocked(uninstallRouteProxyRootCa).mockReset();
    vi.mocked(deleteRouteProxyHttpsCertificates).mockReset();
    vi.mocked(openRouteProxyHttpsCertificateDirectory).mockReset();
    vi.mocked(createMobilePairing).mockResolvedValue({
      v: 1,
      publicUrl: "https://public.example",
      privateUrl: null,
      pairingCode: "pair_test",
      expiresAt: Date.now() + 300000,
    });
    vi.mocked(getWebServiceConfig).mockResolvedValue({
      host: "127.0.0.1",
      port: 3090,
      token: "secret",
      autoStart: false,
      tailscaleEnabled: true,
      tlsEnabled: false,
      tlsCertPath: null,
      tlsKeyPath: null,
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
    vi.mocked(getRouteProxyStatus).mockResolvedValue({
      running: false,
      bind_host: "127.0.0.1",
      port: null,
      base_url: null,
    });
    vi.mocked(getRouteProxyHttpsStatus).mockResolvedValue(httpsStatusFixture);
    vi.mocked(enableRouteProxyHttps).mockResolvedValue(httpsOutcomeFixture);
    vi.mocked(disableRouteProxyHttps).mockResolvedValue(httpsOutcomeFixture);
    vi.mocked(reimportRouteProxyRootCa).mockResolvedValue(httpsOutcomeFixture);
    vi.mocked(regenerateRouteProxyHttpsCertificates).mockResolvedValue(httpsOutcomeFixture);
    vi.mocked(uninstallRouteProxyRootCa).mockResolvedValue(httpsOutcomeFixture);
    vi.mocked(deleteRouteProxyHttpsCertificates).mockResolvedValue(httpsStatusFixture);
    vi.mocked(openRouteProxyHttpsCertificateDirectory).mockResolvedValue();
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

  it("opens the HTTPS settings section and enables the local route proxy", async () => {
    vi.mocked(getSettings).mockResolvedValue(settingsFixture);

    render(
      <QueryClientProvider client={createQueryClient()}>
        <I18nProvider initialLanguage="zh-CN">
          <SettingsScreen />
        </I18nProvider>
      </QueryClientProvider>,
    );

    await userEvent.click(await screen.findByRole("button", { name: /HTTPS/ }));
    expect(await screen.findByRole("heading", { name: "HTTPS" })).toBeInTheDocument();
    expect(screen.getByText("本地算力池 HTTPS")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("checkbox", { name: "为本地算力池启用 HTTPS" }));

    await waitFor(() => expect(enableRouteProxyHttps).toHaveBeenCalledTimes(1));
    expect(await screen.findByText("https://127.0.0.1:8317")).toBeInTheDocument();
  });

  it("shows untrusted guidance without blocking HTTPS controls", async () => {
    vi.mocked(getSettings).mockResolvedValue(settingsFixture);
    vi.mocked(getRouteProxyHttpsStatus).mockResolvedValue({
      ...httpsStatusFixture,
      enabled: true,
      certReady: true,
      manualInstructions: ["certutil.exe -user -addstore Root C:/tmp/root-ca.pem"],
    });

    render(
      <QueryClientProvider client={createQueryClient()}>
        <I18nProvider initialLanguage="zh-CN">
          <SettingsScreen />
        </I18nProvider>
      </QueryClientProvider>,
    );

    await userEvent.click(await screen.findByRole("button", { name: /HTTPS/ }));
    expect(await screen.findByText("根证书尚未受信任")).toBeInTheDocument();
    expect(screen.getByText("certutil.exe -user -addstore Root C:/tmp/root-ca.pem")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "重新导入根证书" })).toBeEnabled();
  });

  it("still surfaces recovery commands when trust could not be verified", async () => {
    // macOS lands here whenever the authorization prompt is dismissed: the
    // import reports success but the trust setting was never written. Gating the
    // panel on "untrusted" used to swallow the commands in exactly this state.
    vi.mocked(getSettings).mockResolvedValue(settingsFixture);
    vi.mocked(getRouteProxyHttpsStatus).mockResolvedValue({
      ...httpsStatusFixture,
      enabled: true,
      certReady: true,
      trustStatus: "unknown" as const,
      message: "Root CA installation completed, but the local trust store could not verify it",
      manualInstructions: [
        "security add-trusted-cert -r trustRoot -k /Users/example/Library/Keychains/login.keychain-db /tmp/root-ca.pem",
      ],
    });

    render(
      <QueryClientProvider client={createQueryClient()}>
        <I18nProvider initialLanguage="zh-CN">
          <SettingsScreen />
        </I18nProvider>
      </QueryClientProvider>,
    );

    await userEvent.click(await screen.findByRole("button", { name: /HTTPS/ }));
    expect(
      await screen.findByText(
        "security add-trusted-cert -r trustRoot -k /Users/example/Library/Keychains/login.keychain-db /tmp/root-ca.pem",
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByText("Root CA installation completed, but the local trust store could not verify it"),
    ).toBeInTheDocument();
  });

  it("preserves advanced Web TLS fields when saving unrelated settings", async () => {
    vi.mocked(getSettings).mockResolvedValue(settingsFixture);
    vi.mocked(getWebServiceConfig).mockResolvedValue({
      host: "127.0.0.1",
      port: 3090,
      token: "secret",
      autoStart: false,
      tailscaleEnabled: false,
      tlsEnabled: true,
      tlsCertPath: " C:/secure/web-cert.pem ",
      tlsKeyPath: " C:/secure/web-key.pem ",
    });

    render(
      <QueryClientProvider client={createQueryClient()}>
        <I18nProvider initialLanguage="zh-CN">
          <SettingsScreen />
        </I18nProvider>
      </QueryClientProvider>,
    );

    await screen.findByRole("heading", { name: "Web 服务" });
    await userEvent.click(await screen.findByRole("button", { name: "保存" }));

    await waitFor(() => expect(saveWebServiceConfig).toHaveBeenCalledTimes(1));
    expect(vi.mocked(saveWebServiceConfig).mock.calls[0][0]).toMatchObject({
      tlsEnabled: true,
      tlsCertPath: "C:/secure/web-cert.pem",
      tlsKeyPath: "C:/secure/web-key.pem",
    });
  });

  it("refreshes secure-network status after starting and stopping Web Service", async () => {
    vi.mocked(getSettings).mockResolvedValue(settingsFixture);

    render(
      <QueryClientProvider client={createQueryClient()}>
        <I18nProvider initialLanguage="en">
          <SettingsScreen />
        </I18nProvider>
      </QueryClientProvider>,
    );

    await userEvent.click(await screen.findByRole("button", { name: "Start service" }));
    await waitFor(() => expect(startWebServer).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(getTailscaleStatus).toHaveBeenCalledTimes(2));

    await userEvent.click(await screen.findByRole("button", { name: "Stop service" }));
    await waitFor(() => expect(stopWebServer).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(getTailscaleStatus).toHaveBeenCalledTimes(3));
  });
});
