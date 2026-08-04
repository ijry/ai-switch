import { useQuery } from "@tanstack/react-query";
import { listPlatformCapabilities } from "../api/client";

export function usePlatformCapabilities() {
  return useQuery({
    queryKey: ["platform-capabilities"],
    queryFn: listPlatformCapabilities,
    staleTime: Infinity,
  });
}
