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
    <section className="space-y-4">
      <div>
        <h1 className="font-display text-3xl font-semibold text-ink">Batches</h1>
        <p className="text-steel">
          Imported providers and official accounts are grouped by batch.
        </p>
      </div>
      <input
        value={search}
        onChange={(event) => setSearch(event.target.value)}
        aria-label="Search batches, accounts, providers"
        placeholder="Search batches, accounts, providers"
        className="w-full rounded-2xl border border-ink/10 bg-white/85 px-4 py-3 text-ink outline-none transition-colors duration-200 placeholder:text-steel/60 focus:border-moss focus:ring-2 focus:ring-moss/20"
      />
      {groupsQuery.isLoading && <p className="text-steel">Loading batches...</p>}
      {groupsQuery.error && <p className="text-ember">Could not load batches.</p>}
      {groupsQuery.data && <BatchList groups={groupsQuery.data} search={deferredSearch} />}
    </section>
  );
}
