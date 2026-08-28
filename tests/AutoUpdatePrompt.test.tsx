import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { AutoUpdatePrompt } from "../src/components/updates/AutoUpdatePrompt";
import { I18nProvider } from "../src/lib/i18n";

const mocks = vi.hoisted(() => ({
  check: vi.fn(),
  relaunch: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-updater", () => ({ check: mocks.check }));
vi.mock("@tauri-apps/plugin-process", () => ({ relaunch: mocks.relaunch }));
vi.mock("../src/lib/transport", () => ({ isDesktop: () => true }));

function renderPrompt(language: "zh-CN" | "en" = "zh-CN") {
  return render(
    <I18nProvider initialLanguage={language}>
      <AutoUpdatePrompt />
    </I18nProvider>,
  );
}

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

describe("AutoUpdatePrompt", () => {
  const updateIntervalMs = 60 * 60 * 1000;

  beforeEach(() => {
    window.localStorage.clear();
    mocks.check.mockReset();
    mocks.relaunch.mockReset();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("checks immediately and every hour while the app is running", async () => {
    vi.useFakeTimers();
    mocks.check.mockResolvedValue({ version: "0.3.0", body: "修复问题" });

    renderPrompt();

    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(screen.getByRole("dialog", { name: "发现新版本" })).toBeInTheDocument();
    expect(screen.getByText("AI Switch 0.3.0 已准备好安装。")).toBeInTheDocument();
    expect(mocks.check).toHaveBeenCalledTimes(1);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(updateIntervalMs - 1);
    });
    expect(mocks.check).toHaveBeenCalledTimes(1);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1);
    });
    expect(mocks.check).toHaveBeenCalledTimes(2);
  });

  it("shows the same release again after dismissal on the next hourly check", async () => {
    vi.useFakeTimers();
    mocks.check.mockResolvedValue({ version: "0.3.0" });

    renderPrompt();

    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });

    const dialog = screen.getByRole("dialog", { name: "发现新版本" });
    const laterButtons = within(dialog).getAllByRole("button", { name: "稍后" });
    fireEvent.click(laterButtons.at(-1)!);
    expect(screen.queryByRole("dialog", { name: "发现新版本" })).not.toBeInTheDocument();

    await act(async () => {
      await vi.advanceTimersByTimeAsync(updateIntervalMs);
    });
    expect(screen.getByRole("dialog", { name: "发现新版本" })).toBeInTheDocument();
  });

  it("does not overlap checks while a request is pending", async () => {
    vi.useFakeTimers();
    let resolveCheck: (value: null) => void = () => undefined;
    mocks.check.mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveCheck = resolve;
        }),
    );

    const view = renderPrompt();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(updateIntervalMs);
    });
    expect(mocks.check).toHaveBeenCalledTimes(1);

    resolveCheck(null);
    await act(async () => {
      await Promise.resolve();
    });
    view.unmount();
  });

  it("cleans up the hourly timer when unmounted", async () => {
    vi.useFakeTimers();
    mocks.check.mockResolvedValue(null);

    const view = renderPrompt();
    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });
    view.unmount();

    await act(async () => {
      await vi.advanceTimersByTimeAsync(updateIntervalMs);
    });
    expect(mocks.check).toHaveBeenCalledTimes(1);
  });

  it("renders the release notes as markdown in the interface language", async () => {
    mocks.check.mockResolvedValue({ version: "0.7.2", body: BILINGUAL_BODY });

    renderPrompt("zh-CN");

    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });

    const dialog = screen.getByRole("dialog", { name: "发现新版本" });
    expect(within(dialog).getByText("修复")).toBeInTheDocument();
    expect(within(dialog).getByRole("listitem")).toHaveTextContent("修正了更新日志显示为空的问题。");
    expect(within(dialog).queryByText("English Release Notes")).not.toBeInTheDocument();
  });

  it("shows the English half when the interface is English", async () => {
    window.localStorage.setItem("ai-switch.language", "en");
    mocks.check.mockResolvedValue({ version: "0.7.2", body: BILINGUAL_BODY });

    renderPrompt("en");

    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });

    const dialog = screen.getByRole("dialog", { name: "A new version is available" });
    expect(within(dialog).getByText("Fixes")).toBeInTheDocument();
    expect(within(dialog).queryByText("中文发布说明")).not.toBeInTheDocument();
  });
});
