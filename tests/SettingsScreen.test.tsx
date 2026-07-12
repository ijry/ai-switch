import { QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { getSettings, saveSettings } from "../src/lib/api/client";
import { createQueryClient } from "../src/lib/query/queryClient";
import { SettingsScreen } from "../src/screens/SettingsScreen";
import { settingsFixture } from "../src/test/fixtures";

vi.mock("../src/lib/api/client", () => ({
  getSettings: vi.fn(),
  saveSettings: vi.fn(),
}));

describe("SettingsScreen", () => {
  beforeEach(() => {
    vi.mocked(getSettings).mockReset();
    vi.mocked(saveSettings).mockReset();
  });

  it("loads settings and saves a toggled theme value", async () => {
    vi.mocked(getSettings).mockResolvedValue(settingsFixture);
    vi.mocked(saveSettings).mockResolvedValue({ ...settingsFixture, theme: "dark" });

    render(
      <QueryClientProvider client={createQueryClient()}>
        <SettingsScreen />
      </QueryClientProvider>,
    );

    expect(await screen.findByText(`Data directory: ${settingsFixture.data_dir}`)).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: /toggle theme value/i }));

    await waitFor(() => expect(saveSettings).toHaveBeenCalled());
    expect(vi.mocked(saveSettings).mock.calls[0][0]).toEqual({ ...settingsFixture, theme: "dark" });
    expect(await screen.findByText("Settings saved.")).toBeInTheDocument();
  });
});
