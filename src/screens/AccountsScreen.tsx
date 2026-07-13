import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Button } from "../components/ui/Button";
import {
  createOfficialAccount,
  listBatchGroups,
  listOfficialAccountStatuses,
  refreshOfficialAccountQuotaSnapshot,
  recordOfficialAccountQuotaSnapshot,
} from "../lib/api/client";
import type { RecordAccountQuotaSnapshotRequest } from "../lib/api/types";

type AccountFormState = {
  platform: string;
  display_name: string;
  email: string;
  plan: string;
  account_metadata_json: string;
  secret_ref: string;
  batch_id: string;
};

type QuotaFormState = {
  status: RecordAccountQuotaSnapshotRequest["status"];
  remaining_label: string;
  reset_at: string;
  summary_json: string;
  raw_excerpt_json: string;
};

const initialFormState: AccountFormState = {
  platform: "codex",
  display_name: "",
  email: "",
  plan: "",
  account_metadata_json: "{}",
  secret_ref: "",
  batch_id: "",
};

const initialQuotaFormState: QuotaFormState = {
  status: "unknown",
  remaining_label: "",
  reset_at: "",
  summary_json: "{}",
  raw_excerpt_json: "{}",
};

export function AccountsScreen() {
  const queryClient = useQueryClient();
  const [form, setForm] = useState<AccountFormState>(initialFormState);
  const [formError, setFormError] = useState<string | null>(null);
  const [selectedQuotaAccountId, setSelectedQuotaAccountId] = useState<string | null>(null);
  const [quotaForm, setQuotaForm] = useState<QuotaFormState>(initialQuotaFormState);
  const [quotaFormError, setQuotaFormError] = useState<string | null>(null);
  const accountsQuery = useQuery({
    queryKey: ["official-account-statuses"],
    queryFn: listOfficialAccountStatuses,
  });
  const batchesQuery = useQuery({
    queryKey: ["batch-groups"],
    queryFn: () => listBatchGroups(),
  });
  const createMutation = useMutation({
    mutationFn: createOfficialAccount,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["official-account-statuses"] });
      queryClient.invalidateQueries({ queryKey: ["official-accounts"] });
      queryClient.invalidateQueries({ queryKey: ["batch-groups"] });
      setForm(initialFormState);
      setFormError(null);
    },
  });
  const quotaMutation = useMutation({
    mutationFn: recordOfficialAccountQuotaSnapshot,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["official-account-statuses"] });
      setSelectedQuotaAccountId(null);
      setQuotaForm(initialQuotaFormState);
      setQuotaFormError(null);
    },
  });
  const quotaRefreshMutation = useMutation({
    mutationFn: refreshOfficialAccountQuotaSnapshot,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["official-account-statuses"] });
    },
  });

  const batches = batchesQuery.data?.map((group) => group.batch) ?? [];
  const accountStatuses = accountsQuery.data ?? [];

  if (accountsQuery.isLoading || batchesQuery.isLoading) {
    return <p className="text-steel">Loading official accounts...</p>;
  }

  if (accountsQuery.error || batchesQuery.error) {
    return <p className="text-ember">Could not load official accounts.</p>;
  }

  return (
    <section className="space-y-4">
      <div>
        <h1 className="font-display text-3xl font-semibold text-ink">Accounts</h1>
        <p className="text-steel">
          Official accounts stay batch-aware and can refresh quota through safe endpoint references.
        </p>
      </div>

      <form
        className="grid gap-4 rounded-3xl border border-ink/10 bg-white/75 p-5 shadow-sm lg:grid-cols-2"
        onSubmit={async (event) => {
          event.preventDefault();
          setFormError(null);

          const normalizedMetadata = form.account_metadata_json.trim() || "{}";
          try {
            JSON.parse(normalizedMetadata);
          } catch {
            setFormError("Account metadata must be valid JSON.");
            return;
          }

          createMutation.mutate({
            account: {
              platform: form.platform.trim(),
              display_name: form.display_name.trim(),
              email: form.email.trim() || null,
              plan: form.plan.trim() || null,
              account_metadata_json: normalizedMetadata,
              secret_ref: form.secret_ref.trim() || null,
            },
            batch_id: form.batch_id || null,
          });
        }}
      >
        <div className="lg:col-span-2">
          <p className="font-display text-xl font-semibold text-ink">Create official account</p>
          <p className="text-sm text-steel">
            Store platform metadata and an optional secret reference.
          </p>
        </div>

        <label className="space-y-2 text-sm font-semibold text-ink">
          <span>Platform</span>
          <input
            value={form.platform}
            onChange={(event) =>
              setForm((current) => ({ ...current, platform: event.target.value }))
            }
            className="w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 text-ink outline-none transition-colors focus:border-moss"
            placeholder="codex"
          />
        </label>

        <label className="space-y-2 text-sm font-semibold text-ink">
          <span>Display name</span>
          <input
            value={form.display_name}
            onChange={(event) =>
              setForm((current) => ({ ...current, display_name: event.target.value }))
            }
            className="w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 text-ink outline-none transition-colors focus:border-moss"
            placeholder="Team Account"
          />
        </label>

        <label className="space-y-2 text-sm font-semibold text-ink">
          <span>Email</span>
          <input
            value={form.email}
            onChange={(event) => setForm((current) => ({ ...current, email: event.target.value }))}
            className="w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 text-ink outline-none transition-colors focus:border-moss"
            placeholder="team@example.com"
          />
        </label>

        <label className="space-y-2 text-sm font-semibold text-ink">
          <span>Plan</span>
          <input
            value={form.plan}
            onChange={(event) => setForm((current) => ({ ...current, plan: event.target.value }))}
            className="w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 text-ink outline-none transition-colors focus:border-moss"
            placeholder="team"
          />
        </label>

        <label className="space-y-2 text-sm font-semibold text-ink">
          <span>Batch</span>
          <select
            value={form.batch_id}
            onChange={(event) =>
              setForm((current) => ({ ...current, batch_id: event.target.value }))
            }
            className="w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 text-ink outline-none transition-colors focus:border-moss"
          >
            <option value="">No batch</option>
            {batches.map((batch) => (
              <option key={batch.id} value={batch.id}>
                {batch.name}
              </option>
            ))}
          </select>
        </label>

        <label className="space-y-2 text-sm font-semibold text-ink">
          <span>Secret ref</span>
          <input
            value={form.secret_ref}
            onChange={(event) =>
              setForm((current) => ({ ...current, secret_ref: event.target.value }))
            }
            className="w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 text-ink outline-none transition-colors focus:border-moss"
            placeholder="secret://account/team"
          />
        </label>

        <label className="space-y-2 text-sm font-semibold text-ink lg:col-span-2">
          <span>Metadata JSON</span>
          <textarea
            value={form.account_metadata_json}
            onChange={(event) =>
              setForm((current) => ({
                ...current,
                account_metadata_json: event.target.value,
              }))
            }
            className="min-h-36 w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 font-mono text-sm text-ink outline-none transition-colors focus:border-moss"
          />
        </label>

        <div className="flex flex-wrap items-center gap-3 lg:col-span-2">
          <Button type="submit" disabled={createMutation.isPending}>
            {createMutation.isPending ? "Creating..." : "Create account"}
          </Button>
          {formError && <p className="text-sm text-ember">{formError}</p>}
          {createMutation.error && !formError && (
            <p className="text-sm text-ember">Could not create official account.</p>
          )}
        </div>
      </form>

      {accountStatuses.length === 0 ? (
        <div className="rounded-3xl border border-dashed border-ink/20 bg-white/70 p-8 text-steel shadow-sm">
          No official accounts yet. Create one above.
        </div>
      ) : (
        <div className="grid gap-3">
          {accountStatuses.map(({ account, quota_snapshot }) => (
            <article
              key={account.id}
              className="rounded-3xl border border-ink/10 bg-white/75 p-5 shadow-sm"
            >
              <div className="flex flex-col gap-2 sm:flex-row sm:items-start sm:justify-between">
                <div>
                  <p className="font-display text-lg font-semibold text-ink">
                    {account.display_name}
                  </p>
                  <p className="text-sm text-steel">{account.platform}</p>
                </div>
                <span className="rounded-full bg-moss/10 px-3 py-1 text-xs font-semibold uppercase tracking-wide text-moss">
                  {account.status}
                </span>
              </div>

              <div className="mt-4 grid gap-2 text-sm text-steel sm:grid-cols-2">
                <p>Email: {account.email ?? "Not set"}</p>
                <p>Plan: {account.plan ?? "Not set"}</p>
                <p className="sm:col-span-2">Secret ref: {account.secret_ref ?? "Not set"}</p>
              </div>

              <div className="mt-4 rounded-2xl bg-paper/70 p-3 text-sm text-steel">
                <p className="font-semibold text-ink">
                  Quota: {quota_snapshot?.status ?? "No quota snapshot"}
                </p>
                {quota_snapshot && (
                  <div className="mt-2 space-y-1">
                    <p>Remaining: {quota_snapshot.remaining_label ?? "Not set"}</p>
                    <p>Reset at: {quota_snapshot.reset_at ?? "Not set"}</p>
                    <p>Fetched at: {quota_snapshot.fetched_at}</p>
                  </div>
                )}
              </div>

              <pre className="mt-4 overflow-x-auto rounded-2xl bg-paper/70 p-3 font-mono text-xs text-ink">
                {account.account_metadata_json}
              </pre>

              <div className="mt-4 flex flex-wrap gap-3">
                <Button
                  type="button"
                  variant="secondary"
                  aria-label={`Record quota for ${account.display_name}`}
                  onClick={() => {
                    setSelectedQuotaAccountId((current) =>
                      current === account.id ? null : account.id,
                    );
                    setQuotaFormError(null);
                  }}
                >
                  Record quota snapshot
                </Button>
                <Button
                  type="button"
                  variant="secondary"
                  aria-label={`Refresh quota for ${account.display_name}`}
                  disabled={quotaRefreshMutation.isPending}
                  onClick={() =>
                    quotaRefreshMutation.mutate({
                      account_id: account.id,
                    })
                  }
                >
                  Refresh quota
                </Button>
              </div>
              {quotaRefreshMutation.error && (
                <p className="mt-3 text-sm text-ember">
                  Could not refresh quota. Configure metadata quota_query first.
                </p>
              )}

              {selectedQuotaAccountId === account.id && (
                <form
                  className="mt-4 grid gap-3 rounded-2xl border border-ink/10 bg-white/70 p-4 sm:grid-cols-2"
                  onSubmit={(event) => {
                    event.preventDefault();
                    setQuotaFormError(null);

                    const summaryJson = quotaForm.summary_json.trim() || "{}";
                    const rawExcerptJson = quotaForm.raw_excerpt_json.trim() || "{}";
                    try {
                      JSON.parse(summaryJson);
                      JSON.parse(rawExcerptJson);
                    } catch {
                      setQuotaFormError("Quota JSON fields must be valid JSON.");
                      return;
                    }

                    quotaMutation.mutate({
                      account_id: account.id,
                      status: quotaForm.status,
                      remaining_label: quotaForm.remaining_label.trim() || null,
                      reset_at: quotaForm.reset_at.trim() || null,
                      summary_json: summaryJson,
                      raw_excerpt_json: rawExcerptJson,
                    });
                  }}
                >
                  <label className="space-y-2 text-sm font-semibold text-ink">
                    <span>Quota status</span>
                    <select
                      value={quotaForm.status}
                      onChange={(event) =>
                        setQuotaForm((current) => ({
                          ...current,
                          status: event.target.value as QuotaFormState["status"],
                        }))
                      }
                      className="w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 text-ink outline-none transition-colors focus:border-moss"
                    >
                      <option value="unknown">unknown</option>
                      <option value="ok">ok</option>
                      <option value="warning">warning</option>
                      <option value="error">error</option>
                    </select>
                  </label>

                  <label className="space-y-2 text-sm font-semibold text-ink">
                    <span>Remaining label</span>
                    <input
                      value={quotaForm.remaining_label}
                      onChange={(event) =>
                        setQuotaForm((current) => ({
                          ...current,
                          remaining_label: event.target.value,
                        }))
                      }
                      className="w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 text-ink outline-none transition-colors focus:border-moss"
                      placeholder="12% remaining"
                    />
                  </label>

                  <label className="space-y-2 text-sm font-semibold text-ink sm:col-span-2">
                    <span>Reset at</span>
                    <input
                      value={quotaForm.reset_at}
                      onChange={(event) =>
                        setQuotaForm((current) => ({ ...current, reset_at: event.target.value }))
                      }
                      className="w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 text-ink outline-none transition-colors focus:border-moss"
                      placeholder="2026-07-14T00:00:00Z"
                    />
                  </label>

                  <label className="space-y-2 text-sm font-semibold text-ink sm:col-span-2">
                    <span>Quota summary JSON</span>
                    <textarea
                      value={quotaForm.summary_json}
                      onChange={(event) =>
                        setQuotaForm((current) => ({
                          ...current,
                          summary_json: event.target.value,
                        }))
                      }
                      className="min-h-24 w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 font-mono text-sm text-ink outline-none transition-colors focus:border-moss"
                    />
                  </label>

                  <label className="space-y-2 text-sm font-semibold text-ink sm:col-span-2">
                    <span>Raw excerpt JSON</span>
                    <textarea
                      value={quotaForm.raw_excerpt_json}
                      onChange={(event) =>
                        setQuotaForm((current) => ({
                          ...current,
                          raw_excerpt_json: event.target.value,
                        }))
                      }
                      className="min-h-24 w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 font-mono text-sm text-ink outline-none transition-colors focus:border-moss"
                    />
                  </label>

                  <div className="flex flex-wrap items-center gap-3 sm:col-span-2">
                    <Button type="submit" disabled={quotaMutation.isPending}>
                      {quotaMutation.isPending ? "Recording..." : "Save quota snapshot"}
                    </Button>
                    {quotaFormError && <p className="text-sm text-ember">{quotaFormError}</p>}
                    {quotaMutation.error && !quotaFormError && (
                      <p className="text-sm text-ember">Could not record quota snapshot.</p>
                    )}
                  </div>
                </form>
              )}
            </article>
          ))}
        </div>
      )}
    </section>
  );
}
