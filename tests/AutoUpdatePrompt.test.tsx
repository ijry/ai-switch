import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AutoUpdatePrompt } from "../src/components/updates/AutoUpdatePrompt";
import { I18nProvider } from "../src/lib/i18n";

const mocks = vi.hoisted(() => ({
  check: vi.fn(),
  relaunch: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-updater", () => ({ check: mocks.check }));
vi.mock("@tauri-apps/plugin-process", () => ({ relaunch: mocks.relaunch }));
vi.mock("../src/lib/transport", () => ({ isDesktop: () => true }));

function renderPrompt() {
  return render(
    <I18nProvider initialLanguage="zh-CN">
      <AutoUpdatePrompt />
    </I18nProvider>,
  );
}

describe("AutoUpdatePrompt", () => {
  beforeEach(() => {
    window.localStorage.clear();
    mocks.check.mockReset();
    mocks.relaunch.mockReset();
  });

  it("checks once per local day and prompts when a release is available", async () => {
    mocks.check.mockResolvedValue({ version: "0.3.0", body: "修复问题" });

    renderPrompt();

    expect(await screen.findByRole("dialog", { name: "发现新版本" })).toBeInTheDocument();
    expect(screen.getByText("AI Switch 0.3.0 已准备好安装。"),).toBeInTheDocument();
    expect(mocks.check).toHaveBeenCalledTimes(1);

    renderPrompt();
    await waitFor(() => expect(mocks.check).toHaveBeenCalledTimes(1));
  });

  it("dismisses the prompt without checking again on the same day", async () => {
    mocks.check.mockResolvedValue({ version: "0.3.0" });

    renderPrompt();
    const dialog = await screen.findByRole("dialog", { name: "发现新版本" });
    const laterButtons = within(dialog).getAllByRole("button", { name: "稍后" });
    fireEvent.click(laterButtons.at(-1)!);

    expect(screen.queryByRole("dialog", { name: "发现新版本" })).not.toBeInTheDocument();
    expect(window.localStorage.getItem("ai-switch.last-update-check")).toBeTruthy();
  });
});
