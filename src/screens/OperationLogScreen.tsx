import { useQuery } from "@tanstack/react-query";
import { listConfigSnapshots } from "../lib/api/client";

export function OperationLogScreen() {
  const snapshotsQuery = useQuery({
    queryKey: ["config-snapshots"],
    queryFn: () => listConfigSnapshots(null, 50),
  });

  return (
    <section>
      <div className="border-b border-stone-200 px-1 pb-3">
        <p className="text-[11px] font-semibold uppercase tracking-wide text-stone-400">Activity</p>
        <h1 className="mt-0.5 text-lg font-semibold text-stone-950">Config Operations</h1>
      </div>
      {snapshotsQuery.isLoading ? <p className="mt-3 text-sm text-stone-500">Loading config operations...</p> : null}
      {snapshotsQuery.error ? (
        <p className="mt-3 text-sm text-red-700" role="alert">
          Could not load config operations.
        </p>
      ) : null}
      {snapshotsQuery.data?.length === 0 ? <p className="mt-3 text-sm text-stone-500">No config operations recorded.</p> : null}
      <div className="mt-3 divide-y divide-stone-200 border-y border-stone-200 bg-white">
        {snapshotsQuery.data?.map((snapshot) => (
          <article className="px-3 py-3" key={snapshot.id}>
            <div className="flex flex-wrap items-center justify-between gap-2">
              <p className="text-[13px] font-semibold text-stone-950">{snapshot.operation}</p>
              <span className="border border-stone-200 bg-stone-50 px-2 py-0.5 font-mono text-[11px] text-stone-700">
                {snapshot.status}
              </span>
            </div>
            <p className="mt-1 break-all font-mono text-[11px] text-stone-600">
              {snapshot.target_app_id ?? "unknown target"} · {snapshot.path}
            </p>
            <p className="mt-1 font-mono text-[11px] text-stone-500">
              {snapshot.created_at} · before {snapshot.before_hash ?? "none"} · after {snapshot.after_hash ?? "none"}
            </p>
            {snapshot.error_code ? <p className="mt-1 font-mono text-[11px] text-red-700">{snapshot.error_code}</p> : null}
          </article>
        ))}
      </div>
    </section>
  );
}
