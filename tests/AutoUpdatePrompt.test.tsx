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

function renderPrompt() {
  return render(
    <I18nProvider initialLanguage="zh-CN">
      <AutoUpdatePrompt />
    </I18nProvider>,
  );
}

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
});
