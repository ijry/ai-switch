import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Button } from "../components/ui/Button";
import {
  createFailoverPolicy,
  createProxyProfile,
  createUsageEvent,
  listFailoverPolicies,
  listProxyProfiles,
  listUsageEvents,
} from "../lib/api/client";
import type { NewFailoverPolicy, NewProxyProfile, NewUsageEvent } from "../lib/api/types";

type ProxyFormState = {
  name: string;
  endpoint_url: string;
  auth_ref: string;
  enabled: boolean;
  notes: string;
};

type FailoverFormState = {
  name: string;
  strategy: NewFailoverPolicy["strategy"];
  provider_ids_json: string;
  enabled: boolean;
  notes: string;
};

type UsageFormState = {
  provider_id: string;
  official_account_id: string;
  source_label: string;
  metric_type: string;
  amount: string;
  unit: string;
  metadata_json: string;
};

const initialProxyForm: ProxyFormState = {
  name: "",
  endpoint_url: "http://127.0.0.1:7890",
  auth_ref: "",
  enabled: true,
  notes: "",
};

const initialFailoverForm: FailoverFormState = {
  name: "",
  strategy: "ordered",
  provider_ids_json: "[\"provider-1\",\"provider-2\"]",
  enabled: true,
  notes: "",
};

const initialUsageForm: UsageFormState = {
  provider_id: "",
  official_account_id: "",
  source_label: "manual",
  metric_type: "request",
  amount: "1",
  unit: "count",
  metadata_json: "{}",
};

export function RoutingScreen() {
  const queryClient = useQueryClient();
  const [proxyForm, setProxyForm] = useState<ProxyFormState>(initialProxyForm);
  const [failoverForm, setFailoverForm] = useState<FailoverFormState>(initialFailoverForm);
  const [usageForm, setUsageForm] = useState<UsageFormState>(initialUsageForm);
  const [failoverError, setFailoverError] = useState<string | null>(null);
  const [usageError, setUsageError] = useState<string | null>(null);

  const proxyQuery = useQuery({ queryKey: ["proxy-profiles"], queryFn: listProxyProfiles });
  const failoverQuery = useQuery({
    queryKey: ["failover-policies"],
    queryFn: listFailoverPolicies,
  });
  const usageQuery = useQuery({ queryKey: ["usage-events"], queryFn: listUsageEvents });

  const proxyMutation = useMutation({
    mutationFn: (request: NewProxyProfile) => createProxyProfile(request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["proxy-profiles"] });
      setProxyForm(initialProxyForm);
    },
  });
  const failoverMutation = useMutation({
    mutationFn: (request: NewFailoverPolicy) => createFailoverPolicy(request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["failover-policies"] });
      setFailoverForm(initialFailoverForm);
      setFailoverError(null);
    },
  });
  const usageMutation = useMutation({
    mutationFn: (request: NewUsageEvent) => createUsageEvent(request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["usage-events"] });
      setUsageForm(initialUsageForm);
      setUsageError(null);
    },
  });

  const proxies = proxyQuery.data ?? [];
  const policies = failoverQuery.data ?? [];
  const usageEvents = usageQuery.data ?? [];
  const isLoading = proxyQuery.isLoading || failoverQuery.isLoading || usageQuery.isLoading;
  const hasError = proxyQuery.error || failoverQuery.error || usageQuery.error;

  if (isLoading) {
    return <p className="text-steel">Loading routing records...</p>;
  }

  if (hasError) {
    return <p className="text-ember">Could not load routing records.</p>;
  }

  return (
    <section className="space-y-4">
      <div>
        <h1 className="font-display text-3xl font-semibold text-ink">Routing</h1>
        <p className="text-steel">
          Manage local proxy metadata, failover policies, and manual usage events without starting
          proxy processes or running failover automatically.
        </p>
      </div>

      <section className="grid gap-4 rounded-3xl border border-ink/10 bg-white/75 p-5 shadow-sm lg:grid-cols-2">
        <div className="lg:col-span-2">
          <h2 className="font-display text-xl font-semibold text-ink">Proxy profile</h2>
          <p className="text-sm text-steel">
            Store endpoint metadata only. Put credentials in `env://` or `secret://` references.
          </p>
        </div>
        <label className="space-y-2 text-sm font-semibold text-ink">
          <span>Proxy name</span>
          <input
            value={proxyForm.name}
            onChange={(event) =>
              setProxyForm((current) => ({ ...current, name: event.target.value }))
            }
            className="w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 text-ink outline-none focus:border-moss"
            placeholder="Local Proxy"
          />
        </label>
        <label className="space-y-2 text-sm font-semibold text-ink">
          <span>Proxy URL</span>
          <input
            value={proxyForm.endpoint_url}
            onChange={(event) =>
              setProxyForm((current) => ({ ...current, endpoint_url: event.target.value }))
            }
            className="w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 text-ink outline-none focus:border-moss"
          />
        </label>
        <label className="space-y-2 text-sm font-semibold text-ink">
          <span>Proxy auth ref</span>
          <input
            value={proxyForm.auth_ref}
            onChange={(event) =>
              setProxyForm((current) => ({ ...current, auth_ref: event.target.value }))
            }
            className="w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 text-ink outline-none focus:border-moss"
            placeholder="env://LOCAL_PROXY_AUTH"
          />
        </label>
        <label className="space-y-2 text-sm font-semibold text-ink">
          <span>Proxy notes</span>
          <input
            value={proxyForm.notes}
            onChange={(event) =>
              setProxyForm((current) => ({ ...current, notes: event.target.value }))
            }
            className="w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 text-ink outline-none focus:border-moss"
          />
        </label>
        <label className="flex items-center gap-2 text-sm font-semibold text-ink">
          <input
            type="checkbox"
            checked={proxyForm.enabled}
            onChange={(event) =>
              setProxyForm((current) => ({ ...current, enabled: event.target.checked }))
            }
          />
          <span>Proxy enabled</span>
        </label>
        <div className="flex items-center gap-3 lg:col-span-2">
          <Button
            type="button"
            disabled={proxyMutation.isPending}
            onClick={() =>
              proxyMutation.mutate({
                name: proxyForm.name.trim(),
                endpoint_url: proxyForm.endpoint_url.trim(),
                auth_ref: proxyForm.auth_ref.trim() || null,
                enabled: proxyForm.enabled,
                notes: proxyForm.notes.trim() || null,
              })
            }
          >
            Create proxy profile
          </Button>
          {proxyMutation.error && <p className="text-sm text-ember">Could not create proxy.</p>}
        </div>
      </section>

      <section className="grid gap-4 rounded-3xl border border-ink/10 bg-white/75 p-5 shadow-sm lg:grid-cols-2">
        <div className="lg:col-span-2">
          <h2 className="font-display text-xl font-semibold text-ink">Failover policy</h2>
          <p className="text-sm text-steel">
            Store ordered provider IDs for future failover execution. D4 does not switch providers
            automatically.
          </p>
        </div>
        <label className="space-y-2 text-sm font-semibold text-ink">
          <span>Failover name</span>
          <input
            value={failoverForm.name}
            onChange={(event) =>
              setFailoverForm((current) => ({ ...current, name: event.target.value }))
            }
            className="w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 text-ink outline-none focus:border-moss"
            placeholder="Primary then backup"
          />
        </label>
        <label className="space-y-2 text-sm font-semibold text-ink">
          <span>Failover strategy</span>
          <select
            value={failoverForm.strategy}
            onChange={(event) =>
              setFailoverForm((current) => ({
                ...current,
                strategy: event.target.value as NewFailoverPolicy["strategy"],
              }))
            }
            className="w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 text-ink outline-none focus:border-moss"
          >
            <option value="ordered">ordered</option>
            <option value="round_robin">round_robin</option>
          </select>
        </label>
        <label className="space-y-2 text-sm font-semibold text-ink lg:col-span-2">
          <span>Provider IDs JSON</span>
          <textarea
            value={failoverForm.provider_ids_json}
            onChange={(event) =>
              setFailoverForm((current) => ({
                ...current,
                provider_ids_json: event.target.value,
              }))
            }
            className="min-h-24 w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 font-mono text-sm text-ink outline-none focus:border-moss"
          />
        </label>
        <label className="space-y-2 text-sm font-semibold text-ink lg:col-span-2">
          <span>Failover notes</span>
          <input
            value={failoverForm.notes}
            onChange={(event) =>
              setFailoverForm((current) => ({ ...current, notes: event.target.value }))
            }
            className="w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 text-ink outline-none focus:border-moss"
          />
        </label>
        <label className="flex items-center gap-2 text-sm font-semibold text-ink">
          <input
            type="checkbox"
            checked={failoverForm.enabled}
            onChange={(event) =>
              setFailoverForm((current) => ({ ...current, enabled: event.target.checked }))
            }
          />
          <span>Failover enabled</span>
        </label>
        <div className="flex flex-wrap items-center gap-3 lg:col-span-2">
          <Button type="button" disabled={failoverMutation.isPending} onClick={submitFailover}>
            Create failover policy
          </Button>
          {failoverError && <p className="text-sm text-ember">{failoverError}</p>}
          {failoverMutation.error && !failoverError && (
            <p className="text-sm text-ember">Could not create failover policy.</p>
          )}
        </div>
      </section>

      <section className="grid gap-4 rounded-3xl border border-ink/10 bg-white/75 p-5 shadow-sm lg:grid-cols-2">
        <div className="lg:col-span-2">
          <h2 className="font-display text-xl font-semibold text-ink">Usage event</h2>
          <p className="text-sm text-steel">
            Record manual usage counters for later dashboards. D4 does not collect usage
            automatically.
          </p>
        </div>
        <label className="space-y-2 text-sm font-semibold text-ink">
          <span>Usage provider ID</span>
          <input
            value={usageForm.provider_id}
            onChange={(event) =>
              setUsageForm((current) => ({ ...current, provider_id: event.target.value }))
            }
            className="w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 text-ink outline-none focus:border-moss"
            placeholder="provider-1"
          />
        </label>
        <label className="space-y-2 text-sm font-semibold text-ink">
          <span>Usage account ID</span>
          <input
            value={usageForm.official_account_id}
            onChange={(event) =>
              setUsageForm((current) => ({
                ...current,
                official_account_id: event.target.value,
              }))
            }
            className="w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 text-ink outline-none focus:border-moss"
            placeholder="account-1"
          />
        </label>
        <label className="space-y-2 text-sm font-semibold text-ink">
          <span>Usage metric</span>
          <select
            value={usageForm.metric_type}
            onChange={(event) =>
              setUsageForm((current) => ({ ...current, metric_type: event.target.value }))
            }
            className="w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 text-ink outline-none focus:border-moss"
          >
            <option value="request">request</option>
            <option value="input_tokens">input_tokens</option>
            <option value="output_tokens">output_tokens</option>
            <option value="total_tokens">total_tokens</option>
            <option value="cost">cost</option>
            <option value="quota">quota</option>
          </select>
        </label>
        <label className="space-y-2 text-sm font-semibold text-ink">
          <span>Usage amount</span>
          <input
            value={usageForm.amount}
            onChange={(event) =>
              setUsageForm((current) => ({ ...current, amount: event.target.value }))
            }
            className="w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 text-ink outline-none focus:border-moss"
            inputMode="numeric"
          />
        </label>
        <label className="space-y-2 text-sm font-semibold text-ink">
          <span>Usage unit</span>
          <input
            value={usageForm.unit}
            onChange={(event) =>
              setUsageForm((current) => ({ ...current, unit: event.target.value }))
            }
            className="w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 text-ink outline-none focus:border-moss"
          />
        </label>
        <label className="space-y-2 text-sm font-semibold text-ink">
          <span>Usage source</span>
          <input
            value={usageForm.source_label}
            onChange={(event) =>
              setUsageForm((current) => ({ ...current, source_label: event.target.value }))
            }
            className="w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 text-ink outline-none focus:border-moss"
          />
        </label>
        <label className="space-y-2 text-sm font-semibold text-ink lg:col-span-2">
          <span>Usage metadata JSON</span>
          <textarea
            value={usageForm.metadata_json}
            onChange={(event) =>
              setUsageForm((current) => ({ ...current, metadata_json: event.target.value }))
            }
            className="min-h-24 w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 font-mono text-sm text-ink outline-none focus:border-moss"
          />
        </label>
        <div className="flex flex-wrap items-center gap-3 lg:col-span-2">
          <Button type="button" disabled={usageMutation.isPending} onClick={submitUsage}>
            Record usage event
          </Button>
          {usageError && <p className="text-sm text-ember">{usageError}</p>}
          {usageMutation.error && !usageError && (
            <p className="text-sm text-ember">Could not record usage event.</p>
          )}
        </div>
      </section>

      <section className="grid gap-4 xl:grid-cols-3">
        <RecordList title="Proxy profiles" empty="No proxy profiles yet.">
          {proxies.map((proxy) => (
            <article key={proxy.id} className="rounded-2xl bg-paper/70 p-4">
              <p className="font-semibold text-ink">{proxy.name}</p>
              <p className="break-all font-mono text-xs text-steel">{proxy.endpoint_url}</p>
              <p className="mt-2 text-xs uppercase tracking-wide text-steel">
                {proxy.enabled === 1 ? "enabled" : "disabled"}
              </p>
              {proxy.notes && <p className="mt-2 text-sm text-steel">{proxy.notes}</p>}
            </article>
          ))}
        </RecordList>

        <RecordList title="Failover policies" empty="No failover policies yet.">
          {policies.map((policy) => (
            <article key={policy.id} className="rounded-2xl bg-paper/70 p-4">
              <p className="font-semibold text-ink">{policy.name}</p>
              <p className="text-sm text-steel">{policy.strategy}</p>
              <pre className="mt-2 overflow-x-auto rounded-xl bg-white/70 p-2 font-mono text-xs text-ink">
                {policy.provider_ids_json}
              </pre>
              {policy.notes && <p className="mt-2 text-sm text-steel">{policy.notes}</p>}
            </article>
          ))}
        </RecordList>

        <RecordList title="Usage events" empty="No usage events yet.">
          {usageEvents.map((event) => (
            <article key={event.id} className="rounded-2xl bg-paper/70 p-4">
              <p className="font-semibold text-ink">
                {event.amount} {event.unit} {event.metric_type}
              </p>
              <p className="text-sm text-steel">{event.source_label}</p>
              {event.provider_id && (
                <p className="break-all font-mono text-xs text-steel">{event.provider_id}</p>
              )}
              <pre className="mt-2 overflow-x-auto rounded-xl bg-white/70 p-2 font-mono text-xs text-ink">
                {event.metadata_json}
              </pre>
            </article>
          ))}
        </RecordList>
      </section>
    </section>
  );

  function submitFailover() {
    setFailoverError(null);
    const providerIdsJson = failoverForm.provider_ids_json.trim() || "[]";
    try {
      const providerIds = JSON.parse(providerIdsJson);
      if (!Array.isArray(providerIds) || providerIds.some((id) => typeof id !== "string")) {
        setFailoverError("Provider IDs JSON must be an array of strings.");
        return;
      }
    } catch {
      setFailoverError("Provider IDs JSON must be valid JSON.");
      return;
    }

    failoverMutation.mutate({
      name: failoverForm.name.trim(),
      strategy: failoverForm.strategy,
      provider_ids_json: providerIdsJson,
      enabled: failoverForm.enabled,
      notes: failoverForm.notes.trim() || null,
    });
  }

  function submitUsage() {
    setUsageError(null);
    const amount = Number(usageForm.amount);
    if (!Number.isInteger(amount) || amount < 0) {
      setUsageError("Usage amount must be a zero or positive integer.");
      return;
    }

    const metadataJson = usageForm.metadata_json.trim() || "{}";
    try {
      const metadata = JSON.parse(metadataJson);
      if (!metadata || Array.isArray(metadata) || typeof metadata !== "object") {
        setUsageError("Usage metadata JSON must be an object.");
        return;
      }
    } catch {
      setUsageError("Usage metadata JSON must be valid JSON.");
      return;
    }

    usageMutation.mutate({
      provider_id: usageForm.provider_id.trim() || null,
      official_account_id: usageForm.official_account_id.trim() || null,
      source_label: usageForm.source_label.trim() || "manual",
      metric_type: usageForm.metric_type,
      amount,
      unit: usageForm.unit.trim(),
      metadata_json: metadataJson,
    });
  }
}

type RecordListProps = {
  title: string;
  empty: string;
  children: React.ReactNode;
};

function RecordList({ title, empty, children }: RecordListProps) {
  const hasChildren = Array.isArray(children) ? children.length > 0 : Boolean(children);

  return (
    <div className="space-y-3 rounded-3xl border border-ink/10 bg-white/75 p-5 shadow-sm">
      <h2 className="font-display text-xl font-semibold text-ink">{title}</h2>
      {hasChildren ? children : <p className="text-sm text-steel">{empty}</p>}
    </div>
  );
}
