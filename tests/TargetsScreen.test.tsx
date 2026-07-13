import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { listTargetSwitchStatuses, rollbackConfigSnapshot } from "../src/lib/api/client";
import { TargetsScreen } from "../src/screens/TargetsScreen";
import { targetSwitchStatusesFixture } from "../src/test/fixtures";

vi.mock("../src/lib/api/client", () => ({
  listTargetSwitchStatuses: vi.fn(),
  rollbackConfigSnapshot: vi.fn(),
}));

function renderWithClient() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
    },
  });

  return render(
    <QueryClientProvider client={queryClient}>
      <TargetsScreen />
    </QueryClientProvider>,
  );
}

describe("TargetsScreen", () => {
  it("shows active provider, write status, and sandbox output path", async () => {
    vi.mocked(listTargetSwitchStatuses).mockResolvedValueOnce(targetSwitchStatusesFixture);

    renderWithClient();

    expect(await screen.findByText("Codex")).toBeInTheDocument();
    expect(screen.getByText("Active provider: Acme Provider")).toBeInTheDocument();
    expect(screen.getByText("Last write: written")).toBeInTheDocument();
    expect(
      screen.getByText("C:/Users/example/.ai-switch/targets/codex/provider.json"),
    ).toBeInTheDocument();
    expect(screen.getAllByText("Active provider: No provider selected")).toHaveLength(2);
  });

  it("rolls back the latest restorable real snapshot", async () => {
    const user = userEvent.setup();
    vi.mocked(listTargetSwitchStatuses).mockResolvedValue([
      {
        ...targetSwitchStatusesFixture[0],
        last_snapshot_operation: "switch_provider:real",
        can_rollback: true,
      },
    ]);
    vi.mocked(rollbackConfigSnapshot).mockResolvedValueOnce({
      target_app_id: "target-codex",
      target_key: "codex",
      source_snapshot_id: "snapshot-1",
      rollback_snapshot_id: "snapshot-rollback",
      state_id: "state-1",
      path: "C:/Users/example/.codex/config.toml",
      status: "rolled_back",
      before_hash: "after",
      after_hash: "before",
      rolled_back_at: "2026-07-13T00:01:00Z",
    });

    renderWithClient();

    await user.click(await screen.findByRole("button", { name: "Restore previous real config" }));

    expect(vi.mocked(rollbackConfigSnapshot).mock.calls[0]?.[0]).toBe("snapshot-1");
  });
});
