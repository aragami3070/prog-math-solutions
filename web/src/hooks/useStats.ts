import { useQuery } from "@tanstack/react-query";
import { get } from "../lib/client";
import type { Statistics } from "../lib/types";

export const STATS_POLL_MS = 5_000;

export function useStats() {
  return useQuery({
    queryKey: ["stats"],
    queryFn: ({ signal }) => get<Statistics>("/api/stats", signal),
    refetchInterval: STATS_POLL_MS,
  });
}
