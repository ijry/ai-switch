import { QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  disableAutostart,
  enableAutostart,
  isAutostartEnabled,
} from "../src/lib/autostart";
import { isDesktop } from "../src/lib/transport";
import { I18nProvider } from "../src/lib/i18n";
import { createQueryClient } from "../src/lib/query/queryClient";
import { AutostartSettings } from "../src/components/settings/autostart-settings";

vi.mock("../src/lib/autostart", () => ({
  disableAutostart: vi.fn(),
  enableAutostart: vi.fn(),
  isAutostartEnabled: vi.fn(),
}));
vi.mock("../src/lib/transport", () => ({ isDesktop: vi.fn() }));

function renderControl() {
  return render(
    <QueryClientProvider client={createQueryClient()}>
      <I18nProvider initialLanguage="zh-CN">
        <AutostartSettings />
      </I18nProvider>
    </QueryClientProvider>,
  );
}

describe("AutostartSettings", () => {
  beforeEach(() => {
    vi.mocked(isDesktop).mockReset();
    vi.mocked(isAutostartEnabled).mockReset();
    vi.mocked(enableAutostart).mockReset();
    vi.mocked(disableAutostart).mockReset();
    vi.mocked(isDesktop).mockReturnValue(true);
    vi.mocked(enableAutostart).mockResolvedValue(undefined);
    vi.mocked(disableAutostart).mockResolvedValue(undefined);
  });

  it("loads the system state and enables autostart", async () => {
    vi.mocked(isAutostartEnabled).mockResolvedValue(false);

    renderControl();
    const checkbox = await screen.findByRole("checkbox", { name: "随系统启动 AI Switch" });
    expect(checkbox).not.toBeChecked();

    await userEvent.click(checkbox);

    await waitFor(() => expect(enableAutostart).toHaveBeenCalledTimes(1));
    expect(checkbox).toBeChecked();
  });

  it("disables autostart and keeps the unchecked state", async () => {
    vi.mocked(isAutostartEnabled).mockResolvedValue(true);

    renderControl();
    const checkbox = await screen.findByRole("checkbox", { name: "随系统启动 AI Switch" });
    await userEvent.click(checkbox);

    await waitFor(() => expect(disableAutostart).toHaveBeenCalledTimes(1));
    expect(checkbox).not.toBeChecked();
  });

  it("keeps the old state and reports a failed update", async () => {
    vi.mocked(isAutostartEnabled).mockResolvedValue(false);
    vi.mocked(enableAutostart).mockRejectedValue(new Error("permission denied"));

    renderControl();
    const checkbox = await screen.findByRole("checkbox", { name: "随系统启动 AI Switch" });
    await userEvent.click(checkbox);

    expect(await screen.findByText("无法更新自启动设置。")).toBeInTheDocument();
    expect(checkbox).not.toBeChecked();
  });

  it("disables the control and reports a failed state read", async () => {
    vi.mocked(isAutostartEnabled).mockRejectedValue(new Error("unavailable"));

    renderControl();

    expect(await screen.findByText("无法读取自启动状态。")).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: "随系统启动 AI Switch" })).toBeDisabled();
  });

  it("does not render or query the plugin in Web runtime", async () => {
    vi.mocked(isDesktop).mockReturnValue(false);

    renderControl();

    expect(screen.queryByRole("checkbox", { name: "随系统启动 AI Switch" })).not.toBeInTheDocument();
    expect(isAutostartEnabled).not.toHaveBeenCalled();
  });
});
