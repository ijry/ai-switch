import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { listTargetSwitchStatuses, rollbackConfigSnapshot } from "../lib/api/client";

export function TargetsScreen() {
  const queryClient = useQueryClient();
  const targetsQuery = useQuery({
    queryKey: ["target-switch-statuses"],
    queryFn: listTargetSwitchStatuses,
  });
  const rollbackMutation = useMutation({
    mutationFn: rollbackConfigSnapshot,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["target-switch-statuses"] });
    },
  });

  return (
    <section className="space-y-4">
      <div>
        <h1 className="font-display text-3xl font-semibold text-ink">Targets</h1>
        <p className="text-steel">Switch state and rollback controls for supported target apps.</p>
      </div>
      {targetsQuery.isLoading && <p className="text-steel">Loading targets...</p>}
      {targetsQuery.error && <p className="text-ember">Could not load targets.</p>}
      <div className="grid gap-3 sm:grid-cols-2">
        {targetsQuery.data?.map((status) => (
          <article
            key={status.target.id}
            className="rounded-3xl border border-ink/10 bg-white/70 p-4 shadow-sm"
          >
            <div className="flex items-start justify-between gap-4">
              <div>
                <p className="font-semibold text-ink">{status.target.display_name}</p>
                <p className="text-sm text-steel">{status.target.key}</p>
              </div>
              <span className="rounded-full bg-white px-3 py-1 text-xs font-semibold text-steel">
                {status.target.enabled ? "Enabled" : "Disabled"}
              </span>
            </div>

            <div className="mt-4 space-y-2 text-sm text-steel">
              <p>Active provider: {status.active_provider?.name ?? "No provider selected"}</p>
              <p>Last write: {status.last_write_status ?? "Never written"}</p>
              {status.last_error_code && (
                <p className="text-ember">Last error: {status.last_error_code}</p>
              )}
              {status.last_written_at && <p>Last written at: {status.last_written_at}</p>}
              {status.last_snapshot_path && (
                <p className="break-all rounded-2xl bg-paper/70 p-3 font-mono text-xs text-ink">
                  {status.last_snapshot_path}
                </p>
              )}
              {status.last_snapshot_operation && (
                <p>Snapshot operation: {status.last_snapshot_operation}</p>
              )}
              {status.can_rollback && status.last_snapshot_id && (
                <button
                  type="button"
                  className="rounded-full bg-ink px-4 py-2 text-xs font-semibold text-white disabled:cursor-not-allowed disabled:bg-steel"
                  disabled={
                    rollbackMutation.isPending &&
                    rollbackMutation.variables === status.last_snapshot_id
                  }
                  onClick={() => rollbackMutation.mutate(status.last_snapshot_id!)}
                >
                  {rollbackMutation.isPending &&
                  rollbackMutation.variables === status.last_snapshot_id
                    ? "Restoring..."
                    : "Restore previous real config"}
                </button>
              )}
              {rollbackMutation.isError &&
                rollbackMutation.variables === status.last_snapshot_id && (
                  <p className="text-ember">Could not restore previous config.</p>
                )}
            </div>
          </article>
        ))}
      </div>
    </section>
  );
}
