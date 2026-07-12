import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { listTargetSwitchStatuses } from "../src/lib/api/client";
import { TargetsScreen } from "../src/screens/TargetsScreen";
import { targetSwitchStatusesFixture } from "../src/test/fixtures";

vi.mock("../src/lib/api/client", () => ({
  listTargetSwitchStatuses: vi.fn(),
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
    expect(screen.getByText("Active provider: No provider selected")).toBeInTheDocument();
  });
});
