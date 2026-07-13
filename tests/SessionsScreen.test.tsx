import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  createSession,
  createSessionEvent,
  listSessionEvents,
  listSessions,
  setSessionStatus,
} from "../src/lib/api/client";
import { SessionsScreen } from "../src/screens/SessionsScreen";
import { sessionEventsFixture, sessionsFixture } from "../src/test/fixtures";

vi.mock("../src/lib/api/client", () => ({
  createSession: vi.fn(),
  createSessionEvent: vi.fn(),
  listSessionEvents: vi.fn(),
  listSessions: vi.fn(),
  setSessionStatus: vi.fn(),
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
      <SessionsScreen />
    </QueryClientProvider>,
  );
}

function mockEmptyLists() {
  vi.mocked(listSessions).mockResolvedValue([]);
  vi.mocked(listSessionEvents).mockResolvedValue([]);
}

describe("SessionsScreen", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("lists sessions and events", async () => {
    vi.mocked(listSessions).mockResolvedValueOnce(sessionsFixture);
    vi.mocked(listSessionEvents).mockResolvedValueOnce(sessionEventsFixture);

    renderWithClient();

    expect(await screen.findByText("Release review")).toBeInTheDocument();
    expect(screen.getByText("Prepare release notes")).toBeInTheDocument();
    expect(screen.getByText("Started review")).toBeInTheDocument();
  });

  it("creates a session", async () => {
    const user = userEvent.setup();
    mockEmptyLists();
    vi.mocked(createSession).mockResolvedValueOnce(sessionsFixture[0]);

    renderWithClient();

    await screen.findByRole("button", { name: "Create session" });
    await user.type(screen.getByLabelText("Session title"), "Release review");
    await user.type(screen.getByLabelText("Target app ID"), "target-codex");
    await user.type(screen.getByLabelText("Provider ID"), "provider-1");
    await user.type(screen.getByLabelText("Official account ID"), "account-1");
    await user.type(screen.getByLabelText("Prompt asset ID"), "prompt-1");
    fireEvent.change(screen.getByLabelText("MCP server IDs JSON"), {
      target: { value: "[\"mcp-1\"]" },
    });
    await user.type(screen.getByLabelText("Session notes"), "Prepare release notes");
    await user.click(screen.getByRole("button", { name: "Create session" }));

    await waitFor(() => expect(createSession).toHaveBeenCalled());
    expect(vi.mocked(createSession).mock.calls[0]?.[0]).toEqual({
      title: "Release review",
      target_app_id: "target-codex",
      provider_id: "provider-1",
      official_account_id: "account-1",
      prompt_asset_id: "prompt-1",
      mcp_server_ids_json: "[\"mcp-1\"]",
      tags_json: "[\"review\"]",
      status: "draft",
      notes: "Prepare release notes",
    });
  });

  it("rejects invalid tags JSON before creating", async () => {
    const user = userEvent.setup();
    mockEmptyLists();

    renderWithClient();

    await screen.findByRole("button", { name: "Create session" });
    fireEvent.change(screen.getByLabelText("Session tags JSON"), {
      target: { value: "[" },
    });
    await user.click(screen.getByRole("button", { name: "Create session" }));

    expect(
      await screen.findByText("Session tags JSON must be an array of strings."),
    ).toBeInTheDocument();
    expect(createSession).not.toHaveBeenCalled();
  });

  it("adds a session event", async () => {
    const user = userEvent.setup();
    mockEmptyLists();
    vi.mocked(createSessionEvent).mockResolvedValueOnce(sessionEventsFixture[0]);

    renderWithClient();

    await screen.findByRole("button", { name: "Add session event" });
    await user.type(screen.getByLabelText("Event session ID"), "session-1");
    await user.type(screen.getByLabelText("Event message"), "Started review");
    await user.click(screen.getByRole("button", { name: "Add session event" }));

    await waitFor(() => expect(createSessionEvent).toHaveBeenCalled());
    expect(vi.mocked(createSessionEvent).mock.calls[0]?.[0]).toEqual({
      session_id: "session-1",
      event_type: "note",
      message: "Started review",
      metadata_json: "{}",
    });
  });

  it("changes session status", async () => {
    const user = userEvent.setup();
    vi.mocked(listSessions).mockResolvedValue(sessionsFixture);
    vi.mocked(listSessionEvents).mockResolvedValue(sessionEventsFixture);
    vi.mocked(setSessionStatus).mockResolvedValueOnce({
      ...sessionsFixture[0],
      status: "active",
    });

    renderWithClient();

    await user.click(await screen.findByRole("button", { name: "Activate Release review" }));

    expect(vi.mocked(setSessionStatus).mock.calls[0]?.[0]).toEqual({
      id: "session-1",
      status: "active",
    });
  });
});
