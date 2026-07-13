import { QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { getSettings, saveSettings } from "../src/lib/api/client";
import { I18nProvider } from "../src/lib/i18n";
import { createQueryClient } from "../src/lib/query/queryClient";
import { SettingsScreen } from "../src/screens/SettingsScreen";
import { settingsFixture } from "../src/test/fixtures";

vi.mock("../src/lib/api/client", () => ({
  getSettings: vi.fn(),
  saveSettings: vi.fn(),
}));

describe("SettingsScreen", () => {
  beforeEach(() => {
    window.localStorage.clear();
    vi.mocked(getSettings).mockReset();
    vi.mocked(saveSettings).mockReset();
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
    expect(screen.getByRole("button", { name: /MCP/ })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /AI 模型/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /导入/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /目标/ })).not.toBeInTheDocument();

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

    await userEvent.click(await screen.findByRole("button", { name: /MCP/ }));
    expect(onOpenFeature).toHaveBeenCalledWith("MCP");
  });
});
