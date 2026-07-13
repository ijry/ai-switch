import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  createUpdateChannel,
  createUpdateCheck,
  listUpdateChannels,
  listUpdateChecks,
} from "../src/lib/api/client";
import { UpdatesScreen } from "../src/screens/UpdatesScreen";
import { updateChannelsFixture, updateChecksFixture } from "../src/test/fixtures";

vi.mock("../src/lib/api/client", () => ({
  createUpdateChannel: vi.fn(),
  createUpdateCheck: vi.fn(),
  listUpdateChannels: vi.fn(),
  listUpdateChecks: vi.fn(),
}));

function renderWithClient() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });

  return render(
    <QueryClientProvider client={queryClient}>
      <UpdatesScreen />
    </QueryClientProvider>,
  );
}

function mockEmptyLists() {
  vi.mocked(listUpdateChannels).mockResolvedValue([]);
  vi.mocked(listUpdateChecks).mockResolvedValue([]);
}

describe("UpdatesScreen", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("lists update channels and checks", async () => {
    vi.mocked(listUpdateChannels).mockResolvedValueOnce(updateChannelsFixture);
    vi.mocked(listUpdateChecks).mockResolvedValueOnce(updateChecksFixture);

    renderWithClient();

    expect(await screen.findByText("Stable")).toBeInTheDocument();
    expect(screen.getByText("Main channel")).toBeInTheDocument();
    expect(screen.getByText("0.1.0 to 0.1.1")).toBeInTheDocument();
  });

  it("creates an update channel", async () => {
    const user = userEvent.setup();
    mockEmptyLists();
    vi.mocked(createUpdateChannel).mockResolvedValueOnce(updateChannelsFixture[0]);

    renderWithClient();

    await screen.findByRole("button", { name: "Create update channel" });
    await user.type(screen.getByLabelText("Channel name"), "Stable");
    await user.type(screen.getByLabelText("Channel notes"), "Main channel");
    await user.click(screen.getByRole("button", { name: "Create update channel" }));

    await waitFor(() => expect(createUpdateChannel).toHaveBeenCalled());
    expect(vi.mocked(createUpdateChannel).mock.calls[0]?.[0]).toEqual({
      name: "Stable",
      channel: "stable",
      feed_url: "https://updates.example.com/stable.json",
      enabled: true,
      notes: "Main channel",
    });
  });

  it("rejects invalid details JSON before recording checks", async () => {
    const user = userEvent.setup();
    mockEmptyLists();

    renderWithClient();

    await screen.findByRole("button", { name: "Record update check" });
    fireEvent.change(screen.getByLabelText("Details JSON"), {
      target: { value: "[" },
    });
    await user.click(screen.getByRole("button", { name: "Record update check" }));

    expect(await screen.findByText("Details JSON must be valid JSON.")).toBeInTheDocument();
    expect(createUpdateCheck).not.toHaveBeenCalled();
  });

  it("records an update check", async () => {
    const user = userEvent.setup();
    mockEmptyLists();
    vi.mocked(createUpdateCheck).mockResolvedValueOnce(updateChecksFixture[0]);

    renderWithClient();

    await screen.findByRole("button", { name: "Record update check" });
    await user.type(screen.getByLabelText("Update channel ID"), "update-channel-1");
    await user.selectOptions(screen.getByLabelText("Check status"), "available");
    await user.type(screen.getByLabelText("Latest version"), "0.1.1");
    await user.type(
      screen.getByLabelText("Release notes URL"),
      "https://updates.example.com/releases/0.1.1",
    );
    await user.click(screen.getByRole("button", { name: "Record update check" }));

    await waitFor(() => expect(createUpdateCheck).toHaveBeenCalled());
    expect(vi.mocked(createUpdateCheck).mock.calls[0]?.[0]).toEqual({
      channel_id: "update-channel-1",
      current_version: "0.1.0",
      latest_version: "0.1.1",
      status: "available",
      release_notes_url: "https://updates.example.com/releases/0.1.1",
      details_json: "{}",
    });
  });
});
