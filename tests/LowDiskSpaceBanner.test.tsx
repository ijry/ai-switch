import { QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { getDiskSpaceStatus } from "../src/lib/api/client";
import { I18nProvider } from "../src/lib/i18n";
import { createQueryClient } from "../src/lib/query/queryClient";
import { DISK_SPACE_REFRESH_MS } from "../src/lib/query/diskSpace";
import { LowDiskSpaceBanner } from "../src/components/system/LowDiskSpaceBanner";
import type { DiskSpaceStatus } from "../src/lib/api/types";

vi.mock("../src/lib/api/client", () => ({ getDiskSpaceStatus: vi.fn() }));

const GIB = 1024 * 1024 * 1024;

function status(availableBytes: number): DiskSpaceStatus {
  return {
    threshold_bytes: GIB,
    low: availableBytes < GIB,
    volumes: [
      {
        label: "C:",
        path: "C:\\",
        total_bytes: 500 * GIB,
        available_bytes: availableBytes,
        low: availableBytes < GIB,
      },
    ],
  };
}

function renderBanner() {
  const queryClient = createQueryClient();
  const view = render(
    <QueryClientProvider client={queryClient}>
      <I18nProvider initialLanguage="zh-CN">
        <LowDiskSpaceBanner />
      </I18nProvider>
    </QueryClientProvider>,
  );
  return { ...view, queryClient };
}

/** Let a resolved query reach the component. */
async function settle() {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(1);
  });
}

/**
 * Fire one polling round and let its result render.
 *
 * React Query hands cache updates to subscribers on a zero-delay timer, so
 * landing exactly on the interval boundary leaves the render pending — the
 * component would then skip straight past the state that round reported.
 */
async function poll() {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(DISK_SPACE_REFRESH_MS);
  });
  await settle();
}

describe("LowDiskSpaceBanner", () => {
  beforeEach(() => {
    vi.mocked(getDiskSpaceStatus).mockReset();
  });

  it("warns with the remaining space once a volume drops below the threshold", async () => {
    vi.mocked(getDiskSpaceStatus).mockResolvedValue(status(640 * 1024 * 1024));

    renderBanner();

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("磁盘空间即将不足");
    expect(alert).toHaveTextContent("剩余空间不足 1 GB");
    expect(alert).toHaveTextContent("C: 剩余 640 MB，共 500 GB");
  });

  it("stays out of the way while there is room", async () => {
    vi.mocked(getDiskSpaceStatus).mockResolvedValue(status(80 * GIB));

    renderBanner();

    await waitFor(() => expect(getDiskSpaceStatus).toHaveBeenCalled());
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("shows nothing when no volume could be probed", async () => {
    vi.mocked(getDiskSpaceStatus).mockResolvedValue({
      threshold_bytes: GIB,
      low: false,
      volumes: [],
    });

    renderBanner();

    await waitFor(() => expect(getDiskSpaceStatus).toHaveBeenCalled());
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("hides the warning once dismissed", async () => {
    vi.mocked(getDiskSpaceStatus).mockResolvedValue(status(120 * 1024 * 1024));

    renderBanner();

    const alert = await screen.findByRole("alert");
    fireEvent.click(screen.getByRole("button", { name: "关闭提示" }));

    expect(alert).not.toBeInTheDocument();
  });

  it("warns again after space is freed and then runs out a second time", async () => {
    vi.useFakeTimers();
    try {
      vi.mocked(getDiskSpaceStatus)
        .mockResolvedValueOnce(status(120 * 1024 * 1024))
        .mockResolvedValueOnce(status(80 * GIB))
        .mockResolvedValue(status(200 * 1024 * 1024));

      renderBanner();
      await settle();
      expect(screen.getByRole("alert")).toBeInTheDocument();

      fireEvent.click(screen.getByRole("button", { name: "关闭提示" }));
      expect(screen.queryByRole("alert")).not.toBeInTheDocument();

      await poll();
      expect(screen.queryByRole("alert")).not.toBeInTheDocument();

      await poll();

      expect(screen.getByRole("alert")).toHaveTextContent("剩余 200 MB");
    } finally {
      vi.useRealTimers();
    }
  });

  it("re-reads free space on the polling interval", async () => {
    vi.useFakeTimers();
    try {
      vi.mocked(getDiskSpaceStatus).mockResolvedValue(status(80 * GIB));

      renderBanner();
      await settle();
      expect(getDiskSpaceStatus).toHaveBeenCalledTimes(1);

      await poll();

      expect(getDiskSpaceStatus).toHaveBeenCalledTimes(2);
    } finally {
      vi.useRealTimers();
    }
  });
});
