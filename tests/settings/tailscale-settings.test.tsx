import { QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  createMobilePairing,
  disconnectTailscale,
  getTailscaleStatus,
  startTailscaleLogin,
  startTailscaleWithAuthKey,
} from "../../src/lib/api/client";
import { TailscaleSettings } from "../../src/components/settings/tailscale-settings";
import { I18nProvider } from "../../src/lib/i18n";
import { createQueryClient } from "../../src/lib/query/queryClient";

vi.mock("../../src/lib/api/client", () => ({
  createMobilePairing: vi.fn(),
  getTailscaleStatus: vi.fn(),
  startTailscaleLogin: vi.fn(),
  startTailscaleWithAuthKey: vi.fn(),
  disconnectTailscale: vi.fn(),
}));

describe("TailscaleSettings", () => {
  beforeEach(() => {
    vi.mocked(getTailscaleStatus).mockReset();
    vi.mocked(createMobilePairing).mockReset();
    vi.mocked(startTailscaleLogin).mockReset();
    vi.mocked(startTailscaleWithAuthKey).mockReset();
    vi.mocked(disconnectTailscale).mockReset();
    vi.mocked(getTailscaleStatus).mockResolvedValue({
      state: "needsLogin",
      deviceName: null,
      tailnetIp: null,
      magicDnsName: null,
      loginUrl: null,
      accessUrls: [],
      serving: false,
      message: null,
    });
    vi.mocked(createMobilePairing).mockResolvedValue({
      v: 1,
      publicUrl: "https://public.example",
      privateUrl: null,
      pairingCode: "pair_test",
      expiresAt: Date.now() + 300000,
    });
    vi.mocked(startTailscaleLogin).mockResolvedValue({
      loginUrl: "https://login.tailscale.com/a/example",
      message: "login started",
    });
    vi.mocked(startTailscaleWithAuthKey).mockResolvedValue({
      state: "connected",
      deviceName: "ai-switch",
      tailnetIp: "100.64.0.12",
      magicDnsName: "ai-switch.tailnet.ts.net",
      accessUrls: ["https://ai-switch.tailnet.ts.net:3090"],
      serving: true,
      message: null,
    });
    vi.mocked(disconnectTailscale).mockResolvedValue({
      state: "notConnected",
      deviceName: null,
      tailnetIp: null,
      accessUrls: [],
      serving: false,
      message: null,
    });
  });

  it("submits auth key and clears the input", async () => {
    render(
      <QueryClientProvider client={createQueryClient()}>
        <I18nProvider initialLanguage="en">
          <TailscaleSettings enabled />
        </I18nProvider>
      </QueryClientProvider>,
    );

    const input = await screen.findByLabelText("Auth key");
    await userEvent.type(input, "tskey-auth-test");
    expect(input).toHaveValue("tskey-auth-test");

    await userEvent.click(screen.getByRole("button", { name: "Connect with auth key" }));

    await waitFor(() => {
      expect(startTailscaleWithAuthKey).toHaveBeenCalledWith("tskey-auth-test");
    });
    expect(input).toHaveValue("");
  });

  it("renders remote access urls when connected", async () => {
    vi.mocked(getTailscaleStatus).mockResolvedValue({
      state: "connected",
      deviceName: "ai-switch",
      tailnetIp: "100.64.0.12",
      magicDnsName: "ai-switch.tailnet.ts.net",
      accessUrls: ["https://ai-switch.tailnet.ts.net:3090"],
      serving: true,
      message: null,
    });

    render(
      <QueryClientProvider client={createQueryClient()}>
        <I18nProvider initialLanguage="en">
          <TailscaleSettings enabled />
        </I18nProvider>
      </QueryClientProvider>,
    );

    expect(await screen.findByText("Remote access")).toBeInTheDocument();
    expect(screen.getByText("https://ai-switch.tailnet.ts.net:3090")).toBeInTheDocument();
    expect(screen.queryByText("100.64.0.12:3090")).not.toBeInTheDocument();
    expect(screen.getByText("Connected")).toBeInTheDocument();
  });

  it("creates a short-lived mobile pairing QR without displaying the Web Service token", async () => {
    vi.mocked(getTailscaleStatus).mockResolvedValue({
      state: "connected",
      deviceName: "ai-switch",
      tailnetIp: "100.64.0.12",
      magicDnsName: "ai-switch.tailnet.ts.net",
      accessUrls: ["https://public.example"],
      serving: true,
      public: true,
      exposureMode: "public",
      message: null,
    });

    render(
      <QueryClientProvider client={createQueryClient()}>
        <I18nProvider initialLanguage="en">
          <TailscaleSettings enabled exposureMode="public" />
        </I18nProvider>
      </QueryClientProvider>,
    );

    await userEvent.click(await screen.findByRole("button", { name: "Show mobile pairing QR" }));
    await waitFor(() => {
      expect(createMobilePairing).toHaveBeenCalledTimes(1);
    });
    expect(await screen.findByAltText("Mobile pairing QR code")).toBeInTheDocument();
    expect(screen.getByText(/pair_test/)).toBeInTheDocument();
    expect(screen.queryByText("secret-web-service-token")).not.toBeInTheDocument();
  });
});
