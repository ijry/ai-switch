import { useQuery } from "@tanstack/react-query";
import { getDiskSpaceStatus } from "../api/client";

/**
 * How often free space is re-read.
 *
 * A disk fills over minutes, not seconds, and the probe is a syscall per volume,
 * so five minutes is frequent enough to warn before writes start failing without
 * polling for its own sake.
 */
export const DISK_SPACE_REFRESH_MS = 5 * 60 * 1000;

export function useDiskSpaceStatus() {
  return useQuery({
    queryKey: ["disk-space-status"],
    queryFn: getDiskSpaceStatus,
    refetchInterval: DISK_SPACE_REFRESH_MS,
    staleTime: DISK_SPACE_REFRESH_MS,
  });
}
