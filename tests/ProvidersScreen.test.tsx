import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { listProviders, listTargetSwitchStatuses, switchTargetProvider } from "../src/lib/api/client";
import { ProvidersScreen } from "../src/screens/ProvidersScreen";
import { providersFixture, targetSwitchStatusesFixture } from "../src/test/fixtures";

vi.mock("../src/lib/api/client", () => ({
  listProviders: vi.fn(),
  listTargetSwitchStatuses: vi.fn(),
  switchTargetProvider: vi.fn(),
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
      <ProvidersScreen />
    </QueryClientProvider>,
  );
}

describe("ProvidersScreen", () => {
  it("shows an empty state when there are no providers", async () => {
    vi.mocked(listProviders).mockResolvedValueOnce([]);
    vi.mocked(listTargetSwitchStatuses).mockResolvedValue(targetSwitchStatusesFixture);

    renderWithClient();

    expect(
      await screen.findByText("No providers yet. Import example JSON to create one."),
    ).toBeInTheDocument();
  });

  it("switches the selected provider to the selected target in sandbox mode", async () => {
    vi.mocked(listProviders).mockResolvedValueOnce(providersFixture);
    vi.mocked(listTargetSwitchStatuses).mockResolvedValue(targetSwitchStatusesFixture);
    vi.mocked(switchTargetProvider).mockResolvedValueOnce({
      target_app_id: "target-codex",
      target_key: "codex",
      provider_id: "provider-1",
      provider_name: "Acme Provider",
      mode: "sandbox",
      path: "C:/Users/example/.ai-switch/targets/codex/provider.json",
      status: "written",
      before_hash: null,
      after_hash: "after",
      snapshot_id: "snapshot-1",
      state_id: "state-1",
      written_at: "2026-07-13T00:00:00Z",
    });

    renderWithClient();

    expect(await screen.findByText("Acme Provider")).toBeInTheDocument();
    await userEvent.selectOptions(screen.getByLabelText("Target for Acme Provider"), "target-codex");
    await userEvent.click(screen.getByRole("button", { name: "Switch Acme Provider in sandbox" }));

    await waitFor(() => {
      expect(switchTargetProvider).toHaveBeenCalledWith({
        target_app_id: "target-codex",
        provider_id: "provider-1",
        mode: "sandbox",
      });
    });
    expect(await screen.findByText("Wrote sandbox config for Acme Provider to Codex.")).toBeInTheDocument();
  });
});
