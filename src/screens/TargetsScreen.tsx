import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ChevronDown, ChevronUp, RotateCcw } from "lucide-react";
import { useState } from "react";
import {
  listConfigSnapshots,
  listTargetConfigStatuses,
  rollbackConfigSnapshot,
} from "../lib/api/client";
import type { ConfigSnapshotSummary, TargetConfigStatus } from "../lib/api/types";

export function TargetsScreen() {
  const statusesQuery = useQuery({
    queryKey: ["target-config-statuses"],
    queryFn: listTargetConfigStatuses,
  });

  return (
    <section className="space-y-3">
      <div className="border-b border-stone-200 px-1 pb-3">
        <p className="text-[11px] font-semibold uppercase tracking-wide text-stone-400">Routing</p>
        <h1 className="mt-0.5 text-lg font-semibold text-stone-950">Targets</h1>
      </div>
      {statusesQuery.isLoading ? <p className="text-sm text-stone-500">Loading target configuration status...</p> : null}
      {statusesQuery.error ? (
        <p className="text-sm text-red-700" role="alert">
          Could not load target configuration status.
        </p>
      ) : null}
      <div className="space-y-2">
        {statusesQuery.data?.map((status) => <TargetStatusRow key={status.target.id} status={status} />)}
      </div>
    </section>
  );
}

function TargetStatusRow({ status }: { status: TargetConfigStatus }) {
  const [expanded, setExpanded] = useState(false);
  const queryClient = useQueryClient();
  const snapshotsQuery = useQuery({
    queryKey: ["config-snapshots", status.target.id],
    queryFn: () => listConfigSnapshots(status.target.id, 50),
    enabled: expanded,
  });
  const rollbackMutation = useMutation({
    mutationFn: (id: string) => rollbackConfigSnapshot(id),
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["target-config-statuses"] }),
        queryClient.invalidateQueries({ queryKey: ["config-snapshots"] }),
      ]);
    },
  });
  const supportLabel = status.support_level === "partial" ? "部分支持" : "已支持";

  return (
    <article className="border border-stone-200 bg-white px-3 py-3 shadow-sm">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            <h2 className="text-[14px] font-semibold text-stone-950">{status.target.display_name}</h2>
            <span
              className={`inline-flex border px-2 py-0.5 text-[10px] font-semibold ${
                status.support_level === "partial"
                  ? "border-amber-200 bg-amber-50 text-amber-800"
                  : "border-emerald-200 bg-emerald-50 text-emerald-800"
              }`}
            >
              {supportLabel}
            </span>
            <span className="font-mono text-[11px] text-stone-500">{status.target.platform ?? status.target.key}</span>
          </div>
          <p className="mt-1 break-all font-mono text-[11px] text-stone-600">
            {status.config_path ?? "No native config adapter registered"}
          </p>
        </div>
        {status.adapter_available ? (
          <button
            aria-expanded={expanded}
            aria-label={`${expanded ? "Hide" : "Show"} snapshots for ${status.target.display_name}`}
            className="inline-flex h-8 w-8 shrink-0 items-center justify-center border border-stone-300 bg-white text-stone-700 hover:bg-stone-50"
            onClick={() => setExpanded((value) => !value)}
            title={`${expanded ? "Hide" : "Show"} snapshots`}
            type="button"
          >
            {expanded ? <ChevronUp className="h-4 w-4" /> : <ChevronDown className="h-4 w-4" />}
          </button>
        ) : null}
      </div>

      <dl className="mt-3 grid gap-x-4 gap-y-1 text-[12px] sm:grid-cols-4">
        <StatusField label="File" value={status.file_status} />
        <StatusField label="Adapter" value={status.adapter_available ? "available" : "unavailable"} />
        <StatusField label="Latest write" value={status.last_write_status ?? "none"} />
        <StatusField label="Snapshots" value={String(status.snapshot_count)} />
      </dl>
      {status.last_error_code ? (
        <p className="mt-2 font-mono text-[11px] text-red-700">{status.last_error_code}</p>
      ) : null}
      {status.latest_snapshot ? (
        <p className="mt-2 font-mono text-[11px] text-stone-500">
          latest {status.latest_snapshot.id} · {status.latest_snapshot.status} · {status.last_written_at ?? status.latest_snapshot.updated_at}
        </p>
      ) : null}

      {expanded ? (
        <div className="mt-3 border-t border-stone-200 pt-3">
          {snapshotsQuery.isLoading ? <p className="text-[12px] text-stone-500">Loading snapshots...</p> : null}
          {snapshotsQuery.error ? (
            <p className="text-[12px] text-red-700" role="alert">
              Could not load snapshots.
            </p>
          ) : null}
          <div className="space-y-2">
            {snapshotsQuery.data?.map((snapshot) => (
              <SnapshotRow
                adapterAvailable={status.adapter_available}
                key={snapshot.id}
                onRollback={() => rollbackMutation.mutate(snapshot.id)}
                rollbackPending={rollbackMutation.isPending}
                snapshot={snapshot}
              />
            ))}
          </div>
          {rollbackMutation.error ? (
            <p className="mt-2 text-[12px] text-red-700" role="alert">
              {errorMessage(rollbackMutation.error)}
            </p>
          ) : null}
        </div>
      ) : null}
    </article>
  );
}

function SnapshotRow({
  adapterAvailable,
  onRollback,
  rollbackPending,
  snapshot,
}: {
  adapterAvailable: boolean;
  onRollback: () => void;
  rollbackPending: boolean;
  snapshot: ConfigSnapshotSummary;
}) {
  const canRollback = adapterAvailable && snapshot.operation === "write" && snapshot.status === "succeeded";

  return (
    <div className="flex flex-wrap items-start justify-between gap-3 border-b border-stone-100 pb-2 text-[12px] last:border-0 last:pb-0">
      <div className="min-w-0">
        <p className="font-medium text-stone-900">
          {snapshot.operation} · {snapshot.status}
        </p>
        <p className="mt-1 break-all font-mono text-[11px] text-stone-500">
          {snapshot.id} · before {snapshot.before_hash ?? "none"} · after {snapshot.after_hash ?? "none"}
        </p>
      </div>
      {canRollback ? (
        <button
          aria-label={`Rollback ${snapshot.id}`}
          className="inline-flex shrink-0 items-center gap-1.5 border border-stone-300 bg-white px-2 py-1 text-[12px] font-medium text-stone-800 hover:bg-stone-50 disabled:cursor-not-allowed disabled:opacity-50"
          disabled={rollbackPending}
          onClick={onRollback}
          type="button"
        >
          <RotateCcw className="h-3.5 w-3.5" />
          Rollback
        </button>
      ) : null}
    </div>
  );
}

function StatusField({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <dt className="text-stone-400">{label}</dt>
      <dd className="font-medium text-stone-700">{value}</dd>
    </div>
  );
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : "Rollback failed.";
}
