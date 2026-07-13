import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Button } from "../components/ui/Button";
import {
  listProviders,
  listTargetSwitchStatuses,
  refreshTrayMenu,
  switchTargetProvider,
} from "../lib/api/client";

const realConfigTargetLabels: Record<string, string> = {
  codex: "Codex",
  opencode: "OpenCode",
};

export function ProvidersScreen() {
  const queryClient = useQueryClient();
  const providersQuery = useQuery({ queryKey: ["providers"], queryFn: listProviders });
  const targetsQuery = useQuery({
    queryKey: ["target-switch-statuses"],
    queryFn: listTargetSwitchStatuses,
  });
  const [selectedTargets, setSelectedTargets] = useState<Record<string, string>>({});
  const switchMutation = useMutation({
    mutationFn: (request: Parameters<typeof switchTargetProvider>[0]) =>
      switchTargetProvider(request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["target-switch-statuses"] });
      queryClient.invalidateQueries({ queryKey: ["targets"] });
      refreshTrayMenu().catch(() => undefined);
    },
  });

  const providers = providersQuery.data ?? [];
  const statuses = targetsQuery.data ?? [];
  const switchedTargetName = switchMutation.data
    ? statuses.find((status) => status.target.id === switchMutation.data.target_app_id)?.target
        .display_name ?? switchMutation.data.target_key
    : null;

  if (providersQuery.isLoading || targetsQuery.isLoading) {
    return <p className="text-steel">Loading providers...</p>;
  }

  if (providersQuery.error || targetsQuery.error) {
    return <p className="text-ember">Could not load provider switching data.</p>;
  }

  if (providers.length === 0) {
    return (
      <section className="rounded-3xl border border-dashed border-ink/20 bg-white/70 p-8 text-center text-steel">
        No providers yet. Import example JSON to create one.
      </section>
    );
  }

  return (
    <section className="space-y-4">
      <div>
        <h1 className="font-display text-3xl font-semibold text-ink">Providers</h1>
        <p className="text-steel">
          Switch a provider into sandbox configs, or write supported user configs explicitly.
        </p>
      </div>

      <div className="grid gap-4">
        {providers.map((provider) => {
          const selectedTargetId = selectedTargets[provider.id] ?? statuses[0]?.target.id ?? "";
          const selectedStatus = statuses.find((status) => status.target.id === selectedTargetId);
          const realConfigTargetLabel = selectedStatus
            ? realConfigTargetLabels[selectedStatus.target.key]
            : undefined;
          const selectId = `target-for-${provider.id}`;

          return (
            <article
              key={provider.id}
              className="rounded-3xl border border-ink/10 bg-white/75 p-5 shadow-sm"
            >
              <div className="flex flex-col gap-4 lg:flex-row lg:items-start lg:justify-between">
                <div>
                  <p className="font-display text-xl font-semibold text-ink">{provider.name}</p>
                  <p className="mt-1 text-sm text-steel">{provider.kind}</p>
                  <p className="mt-2 text-sm text-steel">{provider.base_url ?? "No base URL"}</p>
                  <span className="mt-3 inline-flex rounded-full bg-moss/10 px-3 py-1 text-xs font-semibold uppercase tracking-wide text-moss">
                    {provider.status}
                  </span>
                </div>

                <div className="w-full space-y-3 lg:w-80">
                  <label
                    className="block text-sm font-semibold text-ink"
                    htmlFor={selectId}
                  >
                    Target for {provider.name}
                  </label>
                  <select
                    id={selectId}
                    aria-label={`Target for ${provider.name}`}
                    value={selectedTargetId}
                    onChange={(event) =>
                      setSelectedTargets((current) => ({
                        ...current,
                        [provider.id]: event.target.value,
                      }))
                    }
                    className="w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 text-sm text-ink outline-none transition-colors focus:border-moss"
                  >
                    {statuses.map((status) => (
                      <option key={status.target.id} value={status.target.id}>
                        {status.target.display_name}
                      </option>
                    ))}
                  </select>
                  <Button
                    type="button"
                    disabled={!selectedTargetId || switchMutation.isPending}
                    aria-label={`Switch ${provider.name} in sandbox`}
                    className="cursor-pointer disabled:cursor-not-allowed disabled:opacity-60"
                    onClick={() =>
                      switchMutation.mutate({
                        target_app_id: selectedTargetId,
                        provider_id: provider.id,
                        mode: "sandbox",
                      })
                    }
                  >
                    Switch in sandbox
                  </Button>
                  {realConfigTargetLabel && (
                    <Button
                      type="button"
                      variant="secondary"
                      disabled={!selectedTargetId || switchMutation.isPending}
                      aria-label={`Switch ${provider.name} ${realConfigTargetLabel} config`}
                      className="cursor-pointer disabled:cursor-not-allowed disabled:opacity-60"
                      onClick={() =>
                        switchMutation.mutate({
                          target_app_id: selectedTargetId,
                          provider_id: provider.id,
                          mode: "real",
                        })
                      }
                    >
                      Switch {realConfigTargetLabel} config
                    </Button>
                  )}
                  {selectedStatus?.active_provider && (
                    <p className="text-xs text-steel">
                      Current: {selectedStatus.active_provider.name} on{" "}
                      {selectedStatus.target.display_name}
                    </p>
                  )}
                </div>
              </div>
            </article>
          );
        })}
      </div>

      {switchMutation.data && (
        <p className="rounded-2xl bg-moss/10 p-4 text-sm font-medium text-moss">
          {switchMutation.data.mode === "real"
            ? `Wrote ${switchedTargetName} config`
            : "Wrote sandbox config"}{" "}
          for {switchMutation.data.provider_name} to {switchedTargetName}.
        </p>
      )}
      {switchMutation.error && (
        <p className="rounded-2xl bg-ember/10 p-4 text-sm font-medium text-ember">
          Provider switch failed.
        </p>
      )}
    </section>
  );
}
