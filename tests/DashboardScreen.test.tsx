import { QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  listConfigSnapshots,
  listPlatformCapabilities,
  listTargetConfigStatuses,
} from "../src/lib/api/client";
import { createQueryClient } from "../src/lib/query/queryClient";
import { DashboardScreen } from "../src/screens/DashboardScreen";

vi.mock("../src/lib/api/client", () => ({
  listConfigSnapshots: vi.fn(),
  listPlatformCapabilities: vi.fn(),
  listTargetConfigStatuses: vi.fn(),
}));

describe("DashboardScreen", () => {
  beforeEach(() => {
    vi.mocked(listPlatformCapabilities).mockResolvedValue([
      { platform: "codex", display_name: "Codex", support_level: "supported", operations: {} },
      { platform: "hermes", display_name: "Hermes", support_level: "partial", operations: {} },
    ] as never);
    vi.mocked(listTargetConfigStatuses).mockResolvedValue([
      { adapter_available: true },
      { adapter_available: false },
    ] as never);
    vi.mocked(listConfigSnapshots).mockResolvedValue([
      { status: "succeeded" },
      { status: "conflict" },
      { status: "failed" },
    ] as never);
  });

  it("renders backend-derived counts without an unconditional ready state", async () => {
    render(
      <QueryClientProvider client={createQueryClient()}>
        <DashboardScreen />
      </QueryClientProvider>,
    );

    expect(await screen.findByText("Native adapters")).toBeInTheDocument();
    expect(screen.getByText("Partial platforms")).toBeInTheDocument();
    expect(screen.getByText("Successful config operations")).toBeInTheDocument();
    expect(screen.getByText("Failed or conflicted operations")).toBeInTheDocument();
    expect(screen.getAllByText("1")).toHaveLength(3);
    expect(screen.getAllByText("2")).toHaveLength(1);
    expect(screen.queryByText("Ready")).not.toBeInTheDocument();
  });
});
