import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { ImportPanel } from "../components/imports/ImportPanel";
import {
  createProviderFromPreset,
  exportExampleJson,
  importDeepLink,
  importOfficialAccountJson,
  importExampleJson,
  listProviderPresets,
  refreshTrayMenu,
} from "../lib/api/client";
import type { OfficialAccountJsonImportRequest } from "../lib/api/types";

const defaultAccountImportJson =
  "{\"accounts\":[{\"display_name\":\"Team Codex\",\"email\":\"team@example.com\",\"plan\":\"team\",\"metadata\":{\"workspace\":\"engineering\"},\"secret_ref\":\"secret://account/team\"}]}";

export function ImportsScreen() {
  const queryClient = useQueryClient();
  const [presetBatchName, setPresetBatchName] = useState("Provider presets");
  const [exportJson, setExportJson] = useState<string | null>(null);
  const [deepLinkUrl, setDeepLinkUrl] = useState("");
  const [accountImport, setAccountImport] = useState<OfficialAccountJsonImportRequest>({
    batch_name: "Official accounts",
    source_label: "manual account paste",
    platform: "codex",
    json: defaultAccountImportJson,
  });
  const presetsQuery = useQuery({
    queryKey: ["provider-presets"],
    queryFn: listProviderPresets,
  });
  const importMutation = useMutation({
    mutationFn: (request: Parameters<typeof importExampleJson>[0]) => importExampleJson(request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["batch-groups"] });
      queryClient.invalidateQueries({ queryKey: ["providers"] });
      queryClient.invalidateQueries({ queryKey: ["official-account-statuses"] });
      refreshTrayMenu().catch(() => undefined);
    },
  });
  const accountImportMutation = useMutation({
    mutationFn: (request: OfficialAccountJsonImportRequest) => importOfficialAccountJson(request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["batch-groups"] });
      queryClient.invalidateQueries({ queryKey: ["official-accounts"] });
      queryClient.invalidateQueries({ queryKey: ["official-account-statuses"] });
    },
  });
  const deepLinkImportMutation = useMutation({
    mutationFn: (request: Parameters<typeof importDeepLink>[0]) => importDeepLink(request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["batch-groups"] });
      queryClient.invalidateQueries({ queryKey: ["providers"] });
      queryClient.invalidateQueries({ queryKey: ["official-accounts"] });
      queryClient.invalidateQueries({ queryKey: ["official-account-statuses"] });
      refreshTrayMenu().catch(() => undefined);
    },
  });
  const presetMutation = useMutation({
    mutationFn: (request: Parameters<typeof createProviderFromPreset>[0]) =>
      createProviderFromPreset(request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["batch-groups"] });
      queryClient.invalidateQueries({ queryKey: ["providers"] });
      refreshTrayMenu().catch(() => undefined);
    },
  });
  const exportMutation = useMutation({
    mutationFn: () => exportExampleJson(),
    onSuccess: (outcome) => setExportJson(outcome.json),
  });
  const presets = presetsQuery.data ?? [];

  return (
    <section className="space-y-4">
      <div>
        <h1 className="font-display text-3xl font-semibold text-ink">Imports</h1>
        <p className="text-steel">
          Paste example JSON, seed provider presets, or export data for re-import.
        </p>
      </div>

      <section className="space-y-4 rounded-3xl border border-ink/10 bg-white/75 p-5 shadow-sm shadow-ink/5">
        <div>
          <h2 className="font-display text-xl font-semibold text-ink">Provider presets</h2>
          <p className="text-sm text-steel">
            Create common provider records without storing raw API keys.
          </p>
        </div>
        <label className="block text-sm font-semibold text-ink">
          Preset batch name
          <input
            value={presetBatchName}
            onChange={(event) => setPresetBatchName(event.target.value)}
            className="mt-2 w-full rounded-2xl border border-ink/10 px-4 py-3 outline-none focus:border-moss focus:ring-2 focus:ring-moss/20"
          />
        </label>
        {presetsQuery.isLoading && <p className="text-sm text-steel">Loading presets...</p>}
        {presetsQuery.error && <p className="text-sm text-ember">Could not load presets.</p>}
        <div className="grid gap-3 md:grid-cols-2">
          {presets.map((preset) => (
            <article key={preset.id} className="rounded-2xl bg-paper/70 p-4">
              <p className="font-semibold text-ink">{preset.name}</p>
              <p className="mt-1 text-sm text-steel">{preset.description}</p>
              <p className="mt-2 break-all font-mono text-xs text-steel">
                {preset.base_url ?? "No base URL"}
              </p>
              <button
                type="button"
                disabled={presetMutation.isPending}
                className="mt-3 rounded-full bg-ink px-4 py-2 text-sm font-semibold text-white transition-colors hover:bg-ink/90 disabled:cursor-not-allowed disabled:opacity-60"
                onClick={() =>
                  presetMutation.mutate({
                    preset_id: preset.id,
                    batch_name: presetBatchName.trim() || null,
                  })
                }
              >
                Create {preset.name}
              </button>
            </article>
          ))}
        </div>
        {presetMutation.data && (
          <p className="rounded-2xl bg-moss/10 p-4 text-sm font-medium text-moss">
            Created provider {presetMutation.data.provider.name}.
          </p>
        )}
        {presetMutation.error && <p className="text-sm text-ember">Could not create preset.</p>}
      </section>

      <ImportPanel onImport={(request) => importMutation.mutateAsync(request).then(() => undefined)} />
      {importMutation.data && (
        <p className="rounded-2xl bg-moss/10 p-4 text-moss">
          Imported {importMutation.data.success_count} records into batch {importMutation.data.batch_id}.
        </p>
      )}
      {importMutation.error && <p className="text-ember">Import failed.</p>}

      <section className="space-y-4 rounded-3xl border border-ink/10 bg-white/75 p-5 shadow-sm shadow-ink/5">
        <div>
          <h2 className="font-display text-xl font-semibold text-ink">Deep-link import</h2>
          <p className="text-sm text-steel">
            Paste an ai-switch import link. The app decodes local JSON only; it does not open
            external URLs or run imported content.
          </p>
        </div>
        <label className="block text-sm font-semibold text-ink">
          Deep-link URL
          <textarea
            value={deepLinkUrl}
            onChange={(event) => setDeepLinkUrl(event.target.value)}
            rows={4}
            placeholder="ai-switch://import/example_json?batch_name=Shared&source_label=team&payload=..."
            className="mt-2 w-full rounded-2xl border border-ink/10 px-4 py-3 font-mono text-sm outline-none focus:border-moss focus:ring-2 focus:ring-moss/20"
          />
        </label>
        <button
          type="button"
          disabled={deepLinkImportMutation.isPending || deepLinkUrl.trim().length === 0}
          className="rounded-full bg-ink px-4 py-2 text-sm font-semibold text-white transition-colors hover:bg-ink/90 disabled:cursor-not-allowed disabled:opacity-60"
          onClick={() => deepLinkImportMutation.mutate({ url: deepLinkUrl.trim() })}
        >
          Import deep link
        </button>
        {deepLinkImportMutation.data && (
          <p className="rounded-2xl bg-moss/10 p-4 text-moss">
            Imported {deepLinkImportMutation.data.success_count} deep-link records into batch{" "}
            {deepLinkImportMutation.data.batch_id}.
          </p>
        )}
        {deepLinkImportMutation.error && (
          <p className="text-sm text-ember">Deep-link import failed.</p>
        )}
      </section>

      <section className="space-y-4 rounded-3xl border border-ink/10 bg-white/75 p-5 shadow-sm shadow-ink/5">
        <div>
          <h2 className="font-display text-xl font-semibold text-ink">Official account import</h2>
          <p className="text-sm text-steel">
            Import Codex, Claude, or Gemini account metadata without raw tokens.
          </p>
        </div>
        <div className="grid gap-4 md:grid-cols-2">
          <label className="block text-sm font-semibold text-ink">
            Account batch name
            <input
              value={accountImport.batch_name}
              onChange={(event) =>
                setAccountImport((current) => ({
                  ...current,
                  batch_name: event.target.value,
                }))
              }
              className="mt-2 w-full rounded-2xl border border-ink/10 px-4 py-3 outline-none focus:border-moss focus:ring-2 focus:ring-moss/20"
            />
          </label>
          <label className="block text-sm font-semibold text-ink">
            Account source label
            <input
              value={accountImport.source_label}
              onChange={(event) =>
                setAccountImport((current) => ({
                  ...current,
                  source_label: event.target.value,
                }))
              }
              className="mt-2 w-full rounded-2xl border border-ink/10 px-4 py-3 outline-none focus:border-moss focus:ring-2 focus:ring-moss/20"
            />
          </label>
          <label className="block text-sm font-semibold text-ink">
            Account platform
            <select
              value={accountImport.platform}
              onChange={(event) =>
                setAccountImport((current) => ({
                  ...current,
                  platform: event.target.value as OfficialAccountJsonImportRequest["platform"],
                }))
              }
              className="mt-2 w-full rounded-2xl border border-ink/10 px-4 py-3 outline-none focus:border-moss focus:ring-2 focus:ring-moss/20"
            >
              <option value="codex">Codex</option>
              <option value="claude">Claude</option>
              <option value="gemini">Gemini</option>
              <option value="cursor">Cursor</option>
              <option value="windsurf">Windsurf</option>
              <option value="zed">Zed</option>
              <option value="vscode">VS Code</option>
            </select>
          </label>
        </div>
        <label className="block text-sm font-semibold text-ink">
          Account JSON
          <textarea
            value={accountImport.json}
            onChange={(event) =>
              setAccountImport((current) => ({
                ...current,
                json: event.target.value,
              }))
            }
            rows={8}
            className="mt-2 w-full rounded-2xl border border-ink/10 px-4 py-3 font-mono text-sm outline-none focus:border-moss focus:ring-2 focus:ring-moss/20"
          />
        </label>
        <button
          type="button"
          disabled={accountImportMutation.isPending}
          className="rounded-full bg-ink px-4 py-2 text-sm font-semibold text-white transition-colors hover:bg-ink/90 disabled:cursor-not-allowed disabled:opacity-60"
          onClick={() =>
            accountImportMutation.mutate({
              ...accountImport,
              batch_name: accountImport.batch_name.trim(),
              source_label: accountImport.source_label.trim() || "manual account paste",
            })
          }
        >
          Import official accounts
        </button>
        {accountImportMutation.data && (
          <p className="rounded-2xl bg-moss/10 p-4 text-moss">
            Imported {accountImportMutation.data.success_count} official accounts into batch{" "}
            {accountImportMutation.data.batch_id}.
          </p>
        )}
        {accountImportMutation.error && (
          <p className="text-sm text-ember">Official account import failed.</p>
        )}
      </section>

      <section className="space-y-4 rounded-3xl border border-ink/10 bg-white/75 p-5 shadow-sm shadow-ink/5">
        <div>
          <h2 className="font-display text-xl font-semibold text-ink">Export</h2>
          <p className="text-sm text-steel">
            Export current providers and accounts as example JSON that can be imported again.
          </p>
        </div>
        <button
          type="button"
          disabled={exportMutation.isPending}
          className="rounded-full bg-moss px-4 py-2 text-sm font-semibold text-white transition-colors hover:bg-moss/90 disabled:cursor-not-allowed disabled:opacity-60"
          onClick={() => exportMutation.mutate()}
        >
          Export example JSON
        </button>
        {exportMutation.data && (
          <p className="text-sm text-steel">
            Exported {exportMutation.data.provider_count} providers and{" "}
            {exportMutation.data.account_count} accounts.
          </p>
        )}
        {exportJson && (
          <textarea
            aria-label="Exported example JSON"
            readOnly
            value={exportJson}
            rows={8}
            className="w-full rounded-2xl border border-ink/10 bg-paper/70 px-4 py-3 font-mono text-sm text-ink outline-none"
          />
        )}
        {exportMutation.error && <p className="text-sm text-ember">Export failed.</p>}
      </section>
    </section>
  );
}
