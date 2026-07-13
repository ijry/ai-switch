import { useQuery } from "@tanstack/react-query";
import { listTargetApps } from "../lib/api/client";

export function TargetsScreen() {
  const targetsQuery = useQuery({ queryKey: ["targets"], queryFn: listTargetApps });

  return (
    <section className="space-y-3">
      <div className="rounded-2xl border border-stone-200 bg-white/82 px-4 py-3 shadow-sm">
        <p className="text-[11px] font-semibold uppercase tracking-wide text-stone-400">Routing</p>
        <h1 className="mt-0.5 text-lg font-semibold text-stone-950">Targets</h1>
      </div>
      {targetsQuery.isLoading && <p className="text-sm text-stone-500">Loading targets...</p>}
      {targetsQuery.error && <p className="text-sm text-red-700">Could not load targets.</p>}
      <div className="grid gap-2 sm:grid-cols-2">
        {targetsQuery.data?.map((target) => (
          <div key={target.id} className="rounded-xl border border-stone-200 bg-white/82 px-3 py-2 shadow-sm">
            <p className="text-[13px] font-semibold text-stone-950">{target.display_name}</p>
            <p className="text-[12px] text-stone-500">{target.key}</p>
          </div>
        ))}
      </div>
    </section>
  );
}
