import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import {
  listProviders,
  listTargetSwitchStatuses,
  refreshTrayMenu,
  switchTargetProvider,
} from "../src/lib/api/client";
import { ProvidersScreen } from "../src/screens/ProvidersScreen";
import { providersFixture, targetSwitchStatusesFixture } from "../src/test/fixtures";

vi.mock("../src/lib/api/client", () => ({
  listProviders: vi.fn(),
  listTargetSwitchStatuses: vi.fn(),
  refreshTrayMenu: vi.fn(() =>
    Promise.resolve({ provider_count: 0, target_count: 0, switch_item_count: 0 }),
  ),
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

  it("shows a real Codex switch action when Codex is selected", async () => {
    vi.mocked(listProviders).mockResolvedValueOnce(providersFixture);
    vi.mocked(listTargetSwitchStatuses).mockResolvedValue(targetSwitchStatusesFixture);
    vi.mocked(switchTargetProvider).mockResolvedValueOnce({
      target_app_id: "target-codex",
      target_key: "codex",
      provider_id: "provider-1",
      provider_name: "Acme Provider",
      mode: "real",
      path: "C:/Users/example/.codex/config.toml",
      status: "written",
      before_hash: null,
      after_hash: "after",
      snapshot_id: "snapshot-real",
      state_id: "state-1",
      written_at: "2026-07-13T00:00:00Z",
    });

    renderWithClient();

    expect(await screen.findByText("Acme Provider")).toBeInTheDocument();
    await userEvent.selectOptions(screen.getByLabelText("Target for Acme Provider"), "target-codex");
    await userEvent.click(screen.getByRole("button", { name: "Switch Acme Provider Codex config" }));

    await waitFor(() => {
      expect(switchTargetProvider).toHaveBeenCalledWith({
        target_app_id: "target-codex",
        provider_id: "provider-1",
        mode: "real",
      });
    });
    expect(await screen.findByText("Wrote Codex config for Acme Provider to Codex.")).toBeInTheDocument();
  });

  it("shows a real Claude Code switch action when Claude Code is selected", async () => {
    vi.mocked(listProviders).mockResolvedValueOnce(providersFixture);
    vi.mocked(listTargetSwitchStatuses).mockResolvedValue(targetSwitchStatusesFixture);
    vi.mocked(switchTargetProvider).mockResolvedValueOnce({
      target_app_id: "target-claude",
      target_key: "claude_code",
      provider_id: "provider-1",
      provider_name: "Acme Provider",
      mode: "real",
      path: "C:/Users/example/.claude/settings.json",
      status: "written",
      before_hash: null,
      after_hash: "after",
      snapshot_id: "snapshot-real",
      state_id: "state-1",
      written_at: "2026-07-13T00:00:00Z",
    });

    renderWithClient();

    expect(await screen.findByText("Acme Provider")).toBeInTheDocument();
    await userEvent.selectOptions(screen.getByLabelText("Target for Acme Provider"), "target-claude");
    await userEvent.click(screen.getByRole("button", { name: "Switch Acme Provider Claude Code config" }));

    await waitFor(() => {
      expect(switchTargetProvider).toHaveBeenCalledWith({
        target_app_id: "target-claude",
        provider_id: "provider-1",
        mode: "real",
      });
    });
    expect(await screen.findByText("Wrote Claude Code config for Acme Provider to Claude Code.")).toBeInTheDocument();
  });

  it("shows a real OpenCode switch action when OpenCode is selected", async () => {
    vi.mocked(listProviders).mockResolvedValueOnce(providersFixture);
    vi.mocked(listTargetSwitchStatuses).mockResolvedValue(targetSwitchStatusesFixture);
    vi.mocked(switchTargetProvider).mockResolvedValueOnce({
      target_app_id: "target-opencode",
      target_key: "opencode",
      provider_id: "provider-1",
      provider_name: "Acme Provider",
      mode: "real",
      path: "C:/Users/example/.config/opencode/opencode.json",
      status: "written",
      before_hash: null,
      after_hash: "after",
      snapshot_id: "snapshot-real",
      state_id: "state-1",
      written_at: "2026-07-13T00:00:00Z",
    });

    renderWithClient();

    expect(await screen.findByText("Acme Provider")).toBeInTheDocument();
    await userEvent.selectOptions(screen.getByLabelText("Target for Acme Provider"), "target-opencode");
    await userEvent.click(screen.getByRole("button", { name: "Switch Acme Provider OpenCode config" }));

    await waitFor(() => {
      expect(switchTargetProvider).toHaveBeenCalledWith({
        target_app_id: "target-opencode",
        provider_id: "provider-1",
        mode: "real",
      });
    });
    expect(await screen.findByText("Wrote OpenCode config for Acme Provider to OpenCode.")).toBeInTheDocument();
  });

  it("shows a real Gemini CLI switch action when Gemini CLI is selected", async () => {
    vi.mocked(listProviders).mockResolvedValueOnce(providersFixture);
    vi.mocked(listTargetSwitchStatuses).mockResolvedValue([
      ...targetSwitchStatusesFixture,
      {
        target: {
          id: "target-gemini",
          key: "gemini_cli",
          display_name: "Gemini CLI",
          enabled: 1,
          sort_order: 3,
          created_at: "2026-07-13T00:00:00Z",
          updated_at: "2026-07-13T00:00:00Z",
        },
        active_provider: null,
        last_write_status: null,
        last_error_code: null,
        last_written_at: null,
        last_snapshot_path: null,
        last_snapshot_id: null,
        last_snapshot_operation: null,
        can_rollback: false,
      },
    ]);
    vi.mocked(switchTargetProvider).mockResolvedValueOnce({
      target_app_id: "target-gemini",
      target_key: "gemini_cli",
      provider_id: "provider-1",
      provider_name: "Acme Provider",
      mode: "real",
      path: "C:/Users/example/.gemini/settings.json",
      status: "written",
      before_hash: null,
      after_hash: "after",
      snapshot_id: "snapshot-real",
      state_id: "state-1",
      written_at: "2026-07-13T00:00:00Z",
    });

    renderWithClient();

    expect(await screen.findByText("Acme Provider")).toBeInTheDocument();
    await userEvent.selectOptions(screen.getByLabelText("Target for Acme Provider"), "target-gemini");
    await userEvent.click(screen.getByRole("button", { name: "Switch Acme Provider Gemini CLI config" }));

    await waitFor(() => {
      expect(switchTargetProvider).toHaveBeenCalledWith({
        target_app_id: "target-gemini",
        provider_id: "provider-1",
        mode: "real",
      });
    });
    expect(await screen.findByText("Wrote Gemini CLI config for Acme Provider to Gemini CLI.")).toBeInTheDocument();
  });

  it("hides real switch actions for unsupported targets", async () => {
    vi.mocked(listProviders).mockResolvedValueOnce(providersFixture);
    vi.mocked(listTargetSwitchStatuses).mockResolvedValue([
      ...targetSwitchStatusesFixture,
      {
        target: {
          id: "target-claude-desktop",
          key: "claude_desktop",
          display_name: "Claude Desktop",
          enabled: 1,
          sort_order: 1,
          created_at: "2026-07-13T00:00:00Z",
          updated_at: "2026-07-13T00:00:00Z",
        },
        active_provider: null,
        last_write_status: null,
        last_error_code: null,
        last_written_at: null,
        last_snapshot_path: null,
        last_snapshot_id: null,
        last_snapshot_operation: null,
        can_rollback: false,
      },
    ]);

    renderWithClient();

    expect(await screen.findByText("Acme Provider")).toBeInTheDocument();
    await userEvent.selectOptions(screen.getByLabelText("Target for Acme Provider"), "target-claude-desktop");

    expect(
      screen.queryByRole("button", { name: "Switch Acme Provider Claude Code config" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Switch Acme Provider Codex config" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Switch Acme Provider OpenCode config" }),
    ).not.toBeInTheDocument();
  });
});
