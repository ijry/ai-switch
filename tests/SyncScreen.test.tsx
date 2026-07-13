import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  createSyncProfile,
  createSyncSnapshot,
  listSyncProfiles,
  listSyncSnapshots,
} from "../src/lib/api/client";
import { SyncScreen } from "../src/screens/SyncScreen";
import { syncProfilesFixture, syncSnapshotsFixture } from "../src/test/fixtures";

vi.mock("../src/lib/api/client", () => ({
  createSyncProfile: vi.fn(),
  createSyncSnapshot: vi.fn(),
  listSyncProfiles: vi.fn(),
  listSyncSnapshots: vi.fn(),
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
      <SyncScreen />
    </QueryClientProvider>,
  );
}

function mockEmptyLists() {
  vi.mocked(listSyncProfiles).mockResolvedValue([]);
  vi.mocked(listSyncSnapshots).mockResolvedValue([]);
}

describe("SyncScreen", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("lists sync records", async () => {
    vi.mocked(listSyncProfiles).mockResolvedValueOnce(syncProfilesFixture);
    vi.mocked(listSyncSnapshots).mockResolvedValueOnce(syncSnapshotsFixture);

    renderWithClient();

    expect(await screen.findByText("Team WebDAV")).toBeInTheDocument();
    expect(screen.getAllByText("webdav")).toHaveLength(2);
    expect(screen.getByText("Shared export")).toBeInTheDocument();
    expect(screen.getAllByText("export")).toHaveLength(2);
  });

  it("creates a sync profile", async () => {
    const user = userEvent.setup();
    mockEmptyLists();
    vi.mocked(createSyncProfile).mockResolvedValueOnce(syncProfilesFixture[0]);

    renderWithClient();

    await screen.findByRole("button", { name: "Create sync profile" });
    await user.type(screen.getByLabelText("Profile name"), "Team WebDAV");
    await user.type(screen.getByLabelText("Auth ref"), "env://WEBDAV_TOKEN");
    await user.type(screen.getByLabelText("Profile notes"), "Shared export");
    await user.click(screen.getByRole("button", { name: "Create sync profile" }));

    await waitFor(() => expect(createSyncProfile).toHaveBeenCalled());
    expect(vi.mocked(createSyncProfile).mock.calls[0]?.[0]).toEqual({
      name: "Team WebDAV",
      provider: "webdav",
      endpoint_url: "https://sync.example.com/ai-switch",
      auth_ref: "env://WEBDAV_TOKEN",
      scope_json: "{\"providers\":true,\"accounts\":true,\"routing\":true}",
      enabled: true,
      notes: "Shared export",
    });
  });

  it("rejects invalid scope JSON before creating", async () => {
    const user = userEvent.setup();
    mockEmptyLists();

    renderWithClient();

    await screen.findByRole("button", { name: "Create sync profile" });
    fireEvent.change(screen.getByLabelText("Scope JSON"), {
      target: { value: "[" },
    });
    await user.click(screen.getByRole("button", { name: "Create sync profile" }));

    expect(await screen.findByText("Scope JSON must be valid JSON.")).toBeInTheDocument();
    expect(createSyncProfile).not.toHaveBeenCalled();
  });

  it("records a snapshot manifest", async () => {
    const user = userEvent.setup();
    mockEmptyLists();
    vi.mocked(createSyncSnapshot).mockResolvedValueOnce(syncSnapshotsFixture[0]);

    renderWithClient();

    await screen.findByRole("button", { name: "Record snapshot manifest" });
    await user.type(screen.getByLabelText("Profile ID"), "sync-1");
    await user.click(screen.getByRole("button", { name: "Record snapshot manifest" }));

    await waitFor(() => expect(createSyncSnapshot).toHaveBeenCalled());
    expect(vi.mocked(createSyncSnapshot).mock.calls[0]?.[0]).toEqual({
      profile_id: "sync-1",
      direction: "export",
      artifact_ref: null,
    });
  });
});
