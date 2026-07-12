import { useQuery } from "@tanstack/react-query";
import { listTargetApps } from "../lib/api/client";

export function TargetsScreen() {
  const targetsQuery = useQuery({ queryKey: ["targets"], queryFn: listTargetApps });

  return (
    <section className="space-y-4">
      <h1 className="font-display text-3xl font-semibold text-ink">Targets</h1>
      {targetsQuery.isLoading && <p className="text-steel">Loading targets...</p>}
      {targetsQuery.error && <p className="text-ember">Could not load targets.</p>}
      <div className="grid gap-3 sm:grid-cols-2">
        {targetsQuery.data?.map((target) => (
          <div key={target.id} className="rounded-3xl border border-ink/10 bg-white/70 p-4">
            <p className="font-semibold text-ink">{target.display_name}</p>
            <p className="text-sm text-steel">{target.key}</p>
          </div>
        ))}
      </div>
    </section>
  );
}
