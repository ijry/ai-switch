import { useQuery } from "@tanstack/react-query";
import { useDeferredValue, useState } from "react";
import { BatchList } from "../components/batches/BatchList";
import { listBatchGroups } from "../lib/api/client";

export function BatchesScreen() {
  const [search, setSearch] = useState("");
  const deferredSearch = useDeferredValue(search);
  const groupsQuery = useQuery({
    queryKey: ["batch-groups", deferredSearch],
    queryFn: () => listBatchGroups(deferredSearch),
  });

  return (
    <section className="space-y-3">
      <div className="rounded-2xl border border-stone-200 bg-white/82 shadow-sm">
        <div className="border-b border-stone-200 px-4 py-3">
          <p className="text-[11px] font-semibold uppercase tracking-wide text-stone-400">Library</p>
          <h1 className="mt-0.5 text-lg font-semibold text-stone-950">Batches</h1>
        </div>
        <div className="p-3">
          <input
            value={search}
            onChange={(event) => setSearch(event.target.value)}
            aria-label="Search batches, accounts, providers"
            placeholder="Search batches, accounts, providers"
            className="w-full rounded-xl border border-stone-200 bg-white px-3 py-2 text-[13px] text-stone-900 outline-none transition placeholder:text-stone-400 focus:border-blue-400 focus:ring-2 focus:ring-blue-100"
          />
        </div>
      </div>
      {groupsQuery.isLoading && <p className="text-sm text-stone-500">Loading batches...</p>}
      {groupsQuery.error && <p className="text-sm text-red-700">Could not load batches.</p>}
      {groupsQuery.data && <BatchList groups={groupsQuery.data} search={deferredSearch} />}
    </section>
  );
}
