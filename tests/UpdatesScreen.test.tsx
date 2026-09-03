import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../src/lib/i18n";
import { isDesktop } from "../src/lib/transport";
import { UpdatesScreen } from "../src/screens/UpdatesScreen";

const mocks = vi.hoisted(() => ({ check: vi.fn() }));

vi.mock("@tauri-apps/plugin-updater", () => ({ check: mocks.check }));

vi.mock("@tauri-apps/plugin-process", () => ({
  relaunch: vi.fn(),
}));

vi.mock("../src/lib/transport", () => ({ isDesktop: vi.fn(() => true) }));

const BILINGUAL_BODY = [
  "中文发布说明",
  "",
  "修复",
  "- 修正了更新日志显示为空的问题。",
  "",
  "-".repeat(29),
  "",
  "English Release Notes",
  "",
  "Fixes",
  "- Show the changelog instead of a bare compare link.",
].join("\n");

describe("UpdatesScreen", () => {
  beforeEach(() => {
    window.localStorage.clear();
    mocks.check.mockReset();
    vi.mocked(isDesktop).mockReturnValue(true);
  });

  it("renders the update workflow in Simplified Chinese", () => {
    render(
      <I18nProvider initialLanguage="zh-CN">
        <UpdatesScreen />
      </I18nProvider>,
    );

    expect(screen.getByText("更新")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "应用更新" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "检查更新" })).toBeInTheDocument();
    expect(screen.getByText("尚未选择更新")).toBeInTheDocument();
    expect(screen.getByText("发布来源")).toBeInTheDocument();
  });

  it("renders the release notes as markdown in the interface language", async () => {
    mocks.check.mockResolvedValue({ version: "0.7.2", body: BILINGUAL_BODY, date: undefined });

    render(
      <I18nProvider initialLanguage="zh-CN">
        <UpdatesScreen />
      </I18nProvider>,
    );

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "检查更新" }));
    });

    expect(screen.getByText("修复")).toBeInTheDocument();
    expect(screen.getByRole("listitem")).toHaveTextContent("修正了更新日志显示为空的问题。");
    expect(screen.queryByText("English Release Notes")).not.toBeInTheDocument();
  });

  it("does not call the updater in a browser and says why", async () => {
    // In a browser check() throws "Cannot read properties of undefined", and
    // putting that sentence in the error banner tells the user nothing.
    vi.mocked(isDesktop).mockReturnValue(false);

    render(
      <I18nProvider initialLanguage="zh-CN">
        <UpdatesScreen />
      </I18nProvider>,
    );

    const button = screen.getByRole("button", { name: "检查更新" });
    expect(button).toBeDisabled();
    expect(screen.getByText("此功能仅桌面端可用。")).toBeInTheDocument();

    await act(async () => {
      fireEvent.click(button);
    });

    expect(mocks.check).not.toHaveBeenCalled();
  });
});
