import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  createWakeupRun,
  createWakeupTask,
  listWakeupRuns,
  listWakeupTasks,
  setWakeupTaskEnabled,
} from "../src/lib/api/client";
import { wakeupRunsFixture, wakeupTasksFixture } from "../src/test/fixtures";
import { WakeupsScreen } from "../src/screens/WakeupsScreen";

vi.mock("../src/lib/api/client", () => ({
  createWakeupRun: vi.fn(),
  createWakeupTask: vi.fn(),
  listWakeupRuns: vi.fn(),
  listWakeupTasks: vi.fn(),
  setWakeupTaskEnabled: vi.fn(),
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
      <WakeupsScreen />
    </QueryClientProvider>,
  );
}

describe("WakeupsScreen", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("lists wakeup tasks and run records", async () => {
    vi.mocked(listWakeupTasks).mockResolvedValue(wakeupTasksFixture);
    vi.mocked(listWakeupRuns).mockResolvedValue(wakeupRunsFixture);

    renderWithClient();

    expect(await screen.findByText("Morning review")).toBeInTheDocument();
    expect(screen.getByText("Ready for manual start")).toBeInTheDocument();
    expect(screen.getAllByText("{\"kind\":\"status_record\"}")).toHaveLength(2);
  });

  it("creates a wakeup task", async () => {
    const user = userEvent.setup();
    vi.mocked(listWakeupTasks).mockResolvedValue([]);
    vi.mocked(listWakeupRuns).mockResolvedValue([]);
    vi.mocked(createWakeupTask).mockResolvedValueOnce(wakeupTasksFixture[0]);

    renderWithClient();

    await screen.findByRole("button", { name: "Create wakeup task" });
    await user.type(screen.getByLabelText("Wakeup task name"), "Morning review");
    await user.type(screen.getByLabelText("Wakeup instance ID"), "instance-1");
    await user.type(screen.getByLabelText("Wakeup target app ID"), "target-codex");
    await user.type(screen.getByLabelText("Wakeup provider ID"), "provider-1");
    await user.type(screen.getByLabelText("Wakeup notes"), "Metadata only");
    await user.click(screen.getByRole("button", { name: "Create wakeup task" }));

    await waitFor(() => expect(createWakeupTask).toHaveBeenCalled());
    expect(vi.mocked(createWakeupTask).mock.calls[0]?.[0]).toEqual({
      name: "Morning review",
      managed_instance_id: "instance-1",
      target_app_id: "target-codex",
      provider_id: "provider-1",
      trigger_type: "manual",
      schedule_json: "{\"window\":\"morning\"}",
      action_json: "{\"kind\":\"status_record\"}",
      enabled: true,
      status: "configured",
      notes: "Metadata only",
    });
  });

  it("rejects invalid schedule JSON before creating", async () => {
    const user = userEvent.setup();
    vi.mocked(listWakeupTasks).mockResolvedValue([]);
    vi.mocked(listWakeupRuns).mockResolvedValue([]);

    renderWithClient();

    await screen.findByRole("button", { name: "Create wakeup task" });
    fireEvent.change(screen.getByLabelText("Wakeup schedule JSON"), {
      target: { value: "[]" },
    });
    await user.click(screen.getByRole("button", { name: "Create wakeup task" }));

    expect(await screen.findByText("Wakeup schedule JSON must be an object.")).toBeInTheDocument();
    expect(createWakeupTask).not.toHaveBeenCalled();
  });

  it("records a wakeup run", async () => {
    const user = userEvent.setup();
    vi.mocked(listWakeupTasks).mockResolvedValue(wakeupTasksFixture);
    vi.mocked(listWakeupRuns).mockResolvedValue([]);
    vi.mocked(createWakeupRun).mockResolvedValueOnce(wakeupRunsFixture[0]);

    renderWithClient();

    await screen.findByText("Morning review");
    await user.type(screen.getByLabelText("Wakeup run task ID"), "wakeup-task-1");
    await user.type(screen.getByLabelText("Wakeup run message"), "Ready for manual start");
    await user.click(screen.getByRole("button", { name: "Record wakeup run" }));

    await waitFor(() => expect(createWakeupRun).toHaveBeenCalled());
    expect(vi.mocked(createWakeupRun).mock.calls[0]?.[0]).toEqual({
      task_id: "wakeup-task-1",
      outcome: "recorded",
      message: "Ready for manual start",
      metadata_json: "{}",
    });
  });

  it("toggles wakeup task enabled state", async () => {
    const user = userEvent.setup();
    vi.mocked(listWakeupTasks).mockResolvedValue(wakeupTasksFixture);
    vi.mocked(listWakeupRuns).mockResolvedValue([]);
    vi.mocked(setWakeupTaskEnabled).mockResolvedValueOnce({
      ...wakeupTasksFixture[0],
      enabled: 0,
      status: "paused",
    });

    renderWithClient();

    await user.click(await screen.findByRole("button", { name: "Disable Morning review" }));

    expect(vi.mocked(setWakeupTaskEnabled).mock.calls[0]?.[0]).toEqual({
      id: "wakeup-task-1",
      enabled: false,
    });
  });
});
