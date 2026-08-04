import { QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { listConfigSnapshots } from "../src/lib/api/client";
import { createQueryClient } from "../src/lib/query/queryClient";
import { OperationLogScreen } from "../src/screens/OperationLogScreen";

vi.mock("../src/lib/api/client", () => ({ listConfigSnapshots: vi.fn() }));

describe("OperationLogScreen", () => {
  beforeEach(() => {
    vi.mocked(listConfigSnapshots).mockResolvedValue([
      {
        id: "snapshot-1",
        target_app_id: "target-codex",
        platform: "codex",
        operation: "write",
        operation_group_id: "operation-1",
        source_snapshot_id: null,
        path: "C:\\Users\\test\\.codex\\config.toml",
        before_hash: "before-hash",
        after_hash: "after-hash",
        original_file_existed: 1,
        status: "succeeded",
        error_code: null,
        created_at: "2026-08-04T00:00:00Z",
        updated_at: "2026-08-04T00:00:01Z",
      },
    ]);
  });

  it("renders only returned config operations without import-event claims", async () => {
    render(
      <QueryClientProvider client={createQueryClient()}>
        <OperationLogScreen />
      </QueryClientProvider>,
    );

    expect(await screen.findByText("Config Operations")).toBeInTheDocument();
    expect(await screen.findByText("write")).toBeInTheDocument();
    expect(screen.getByText("succeeded")).toBeInTheDocument();
    expect(screen.getByText(/target-codex/)).toBeInTheDocument();
    expect(screen.getByText(/before-hash/)).toBeInTheDocument();
    expect(screen.queryByText(/Import and config write events/)).not.toBeInTheDocument();
  });
});
