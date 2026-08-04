import { QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  listConfigSnapshots,
  listTargetConfigStatuses,
  rollbackConfigSnapshot,
} from "../src/lib/api/client";
import { createQueryClient } from "../src/lib/query/queryClient";
import { TargetsScreen } from "../src/screens/TargetsScreen";
import type { ConfigSnapshotSummary, TargetConfigStatus } from "../src/lib/api/types";

vi.mock("../src/lib/api/client", () => ({
  listConfigSnapshots: vi.fn(),
  listTargetConfigStatuses: vi.fn(),
  rollbackConfigSnapshot: vi.fn(),
}));

const codexSnapshot: ConfigSnapshotSummary = {
  id: "snapshot-codex-1",
  target_app_id: "target-codex",
  platform: "codex",
  operation: "write",
  operation_group_id: "operation-codex-1",
  source_snapshot_id: null,
  path: "C:\\Users\\test\\.codex\\config.toml",
  before_hash: "before-hash",
  after_hash: "after-hash",
  original_file_existed: 1,
  status: "succeeded",
  error_code: null,
  created_at: "2026-08-04T00:00:00Z",
  updated_at: "2026-08-04T00:00:01Z",
};

const statuses: TargetConfigStatus[] = [
  {
    target: {
      id: "target-codex",
      key: "codex",
      platform: "codex",
      display_name: "Codex",
      enabled: 1,
      sort_order: 0,
      created_at: "2026-08-04T00:00:00Z",
      updated_at: "2026-08-04T00:00:00Z",
    },
    support_level: "supported",
    adapter_available: true,
    config_path: codexSnapshot.path,
    file_status: "managed",
    last_write_status: "succeeded",
    last_error_code: null,
    last_written_at: "2026-08-04T00:00:01Z",
    snapshot_count: 1,
    latest_snapshot: codexSnapshot,
  },
  {
    target: {
      id: "target-hermes",
      key: "hermes",
      platform: "hermes",
      display_name: "Hermes",
      enabled: 1,
      sort_order: 1,
      created_at: "2026-08-04T00:00:00Z",
      updated_at: "2026-08-04T00:00:00Z",
    },
    support_level: "partial",
    adapter_available: false,
    config_path: null,
    file_status: "adapter_unavailable",
    last_write_status: null,
    last_error_code: null,
    last_written_at: null,
    snapshot_count: 0,
    latest_snapshot: null,
  },
];

function renderScreen() {
  const queryClient = createQueryClient();
  return {
    queryClient,
    ...render(
      <QueryClientProvider client={queryClient}>
        <TargetsScreen />
      </QueryClientProvider>,
    ),
  };
}

describe("TargetsScreen", () => {
  beforeEach(() => {
    vi.mocked(listTargetConfigStatuses).mockResolvedValue(statuses);
    vi.mocked(listConfigSnapshots).mockResolvedValue([codexSnapshot]);
    vi.mocked(rollbackConfigSnapshot).mockResolvedValue({
      operation_id: "rollback-operation-1",
      snapshot_id: "rollback-snapshot-1",
      target_app_id: "target-codex",
      target_key: "codex",
      platform: "codex",
      path: codexSnapshot.path,
      status: "succeeded",
      before_hash: "after-hash",
      after_hash: "before-hash",
      error_code: null,
    });
  });

  it("shows managed target metadata and omits unavailable Hermes config actions", async () => {
    renderScreen();

    expect(await screen.findByText("Codex")).toBeInTheDocument();
    expect(screen.getByText(codexSnapshot.path)).toBeInTheDocument();
    expect(screen.getByText("managed")).toBeInTheDocument();
    expect(screen.getByText("succeeded")).toBeInTheDocument();
    expect(screen.getByText("Hermes")).toBeInTheDocument();
    expect(screen.getByText("adapter_unavailable")).toBeInTheDocument();
    expect(screen.getByText("部分支持")).toBeInTheDocument();
    expect(screen.queryByText(".hermes\\config.yaml")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Hermes.*Rollback/ })).not.toBeInTheDocument();
  });

  it("rolls back successful write snapshots and invalidates status queries", async () => {
    const { queryClient } = renderScreen();
    const invalidateQueries = vi.spyOn(queryClient, "invalidateQueries");
    await screen.findByText("Codex");

    await userEvent.click(screen.getByRole("button", { name: "Show snapshots for Codex" }));
    await userEvent.click(await screen.findByRole("button", { name: "Rollback snapshot-codex-1" }));

    await waitFor(() => expect(rollbackConfigSnapshot).toHaveBeenCalledWith("snapshot-codex-1"));
    await waitFor(() => expect(invalidateQueries).toHaveBeenCalledTimes(2));
  });

  it("shows a rollback conflict as an alert", async () => {
    vi.mocked(rollbackConfigSnapshot).mockRejectedValue(new Error("config.rollback_conflict"));
    renderScreen();
    await screen.findByText("Codex");

    await userEvent.click(screen.getByRole("button", { name: "Show snapshots for Codex" }));
    await userEvent.click(await screen.findByRole("button", { name: "Rollback snapshot-codex-1" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("config.rollback_conflict");
  });
});
