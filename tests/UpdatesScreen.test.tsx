import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../src/lib/i18n";
import { UpdatesScreen } from "../src/screens/UpdatesScreen";

vi.mock("@tauri-apps/plugin-updater", () => ({
  check: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-process", () => ({
  relaunch: vi.fn(),
}));

describe("UpdatesScreen", () => {
  beforeEach(() => {
    window.localStorage.clear();
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
});
