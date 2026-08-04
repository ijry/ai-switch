import { useQuery } from "@tanstack/react-query";
import {
  listConfigSnapshots,
  listPlatformCapabilities,
  listTargetConfigStatuses,
} from "../lib/api/client";

export function DashboardScreen() {
  const capabilitiesQuery = useQuery({ queryKey: ["platform-capabilities"], queryFn: listPlatformCapabilities });
  const statusesQuery = useQuery({ queryKey: ["target-config-statuses"], queryFn: listTargetConfigStatuses });
  const snapshotsQuery = useQuery({ queryKey: ["config-snapshots"], queryFn: () => listConfigSnapshots(null, 50) });
  const cards = [
    statusesQuery.data
      ? {
          label: "Native adapters",
          value: statusesQuery.data.filter((status) => status.adapter_available).length,
        }
      : null,
    capabilitiesQuery.data
      ? {
          label: "Partial platforms",
          value: capabilitiesQuery.data.filter((capability) => capability.support_level === "partial").length,
        }
      : null,
    snapshotsQuery.data
      ? {
          label: "Successful config operations",
          value: snapshotsQuery.data.filter((snapshot) => snapshot.status === "succeeded").length,
        }
      : null,
    snapshotsQuery.data
      ? {
          label: "Failed or conflicted operations",
          value: snapshotsQuery.data.filter((snapshot) => ["failed", "conflict"].includes(snapshot.status)).length,
        }
      : null,
  ].filter((card): card is { label: string; value: number } => card !== null);

  return (
    <section>
      <div className="border-b border-stone-200 px-1 pb-3">
        <p className="text-[11px] font-semibold uppercase tracking-wide text-stone-400">Overview</p>
        <h1 className="mt-0.5 text-lg font-semibold text-stone-950">AI Switch</h1>
      </div>
      <div className="mt-3 grid gap-2 sm:grid-cols-2 xl:grid-cols-4">
        {cards.map((card) => (
          <div className="border border-stone-200 bg-white px-3 py-3 shadow-sm" key={card.label}>
            <p className="text-[12px] font-medium text-stone-600">{card.label}</p>
            <p className="mt-1 font-mono text-xl font-semibold text-stone-950">{card.value}</p>
          </div>
        ))}
      </div>
      {cards.length === 0 ? <p className="mt-3 text-sm text-stone-500">Loading current configuration data...</p> : null}
    </section>
  );
}
