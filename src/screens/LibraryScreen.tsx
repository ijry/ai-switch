import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Button } from "../components/ui/Button";
import {
  createPromptAsset,
  listPromptAssets,
  setPromptAssetEnabled,
} from "../lib/api/client";
import type {
  NewPromptAsset,
  PromptAssetType,
  SetPromptAssetEnabledRequest,
} from "../lib/api/types";

type LibraryFormState = {
  item_type: PromptAssetType;
  name: string;
  description: string;
  body: string;
  tags_json: string;
  metadata_json: string;
  enabled: boolean;
};

const initialFormState: LibraryFormState = {
  item_type: "prompt",
  name: "",
  description: "",
  body: "",
  tags_json: "[]",
  metadata_json: "{}",
  enabled: true,
};

export function LibraryScreen() {
  const queryClient = useQueryClient();
  const [form, setForm] = useState<LibraryFormState>(initialFormState);
  const [formError, setFormError] = useState<string | null>(null);
  const assetsQuery = useQuery({
    queryKey: ["prompt-assets"],
    queryFn: listPromptAssets,
  });
  const createMutation = useMutation({
    mutationFn: (request: NewPromptAsset) => createPromptAsset(request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["prompt-assets"] });
      setForm(initialFormState);
      setFormError(null);
    },
  });
  const toggleMutation = useMutation({
    mutationFn: (request: SetPromptAssetEnabledRequest) => setPromptAssetEnabled(request),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["prompt-assets"] }),
  });
  const assets = assetsQuery.data ?? [];

  if (assetsQuery.isLoading) {
    return <p className="text-steel">Loading library...</p>;
  }

  if (assetsQuery.error) {
    return <p className="text-ember">Could not load prompt library.</p>;
  }

  return (
    <section className="space-y-4">
      <div>
        <h1 className="font-display text-3xl font-semibold text-ink">Library</h1>
        <p className="text-steel">
          Store reusable prompts and skill instructions locally without executing them or writing
          target app config.
        </p>
      </div>

      <form
        className="grid gap-4 rounded-3xl border border-ink/10 bg-white/75 p-5 shadow-sm lg:grid-cols-2"
        onSubmit={(event) => {
          event.preventDefault();
          setFormError(null);

          const tagsJson = form.tags_json.trim() || "[]";
          const metadataJson = form.metadata_json.trim() || "{}";
          try {
            const tags = JSON.parse(tagsJson);
            if (!Array.isArray(tags) || tags.some((tag) => typeof tag !== "string")) {
              setFormError("Tags JSON must be an array of strings.");
              return;
            }
          } catch {
            setFormError("Tags JSON must be valid JSON.");
            return;
          }

          try {
            const metadata = JSON.parse(metadataJson);
            if (!metadata || Array.isArray(metadata) || typeof metadata !== "object") {
              setFormError("Metadata JSON must be an object.");
              return;
            }
          } catch {
            setFormError("Metadata JSON must be valid JSON.");
            return;
          }

          createMutation.mutate({
            item_type: form.item_type,
            name: form.name.trim(),
            description: form.description.trim() || null,
            body: form.body.trim(),
            tags_json: tagsJson,
            metadata_json: metadataJson,
            enabled: form.enabled,
          });
        }}
      >
        <div className="lg:col-span-2">
          <p className="font-display text-xl font-semibold text-ink">Create library item</p>
          <p className="text-sm text-steel">
            Prompts and skills are local records only. Do not paste raw credentials into the body.
          </p>
        </div>

        <label className="space-y-2 text-sm font-semibold text-ink">
          <span>Type</span>
          <select
            value={form.item_type}
            onChange={(event) =>
              setForm((current) => ({
                ...current,
                item_type: event.target.value as PromptAssetType,
              }))
            }
            className="w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 text-ink outline-none transition-colors focus:border-moss"
          >
            <option value="prompt">prompt</option>
            <option value="skill">skill</option>
          </select>
        </label>

        <label className="space-y-2 text-sm font-semibold text-ink">
          <span>Name</span>
          <input
            value={form.name}
            onChange={(event) => setForm((current) => ({ ...current, name: event.target.value }))}
            className="w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 text-ink outline-none transition-colors focus:border-moss"
            placeholder="Review Prompt"
          />
        </label>

        <label className="space-y-2 text-sm font-semibold text-ink lg:col-span-2">
          <span>Description</span>
          <input
            value={form.description}
            onChange={(event) =>
              setForm((current) => ({ ...current, description: event.target.value }))
            }
            className="w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 text-ink outline-none transition-colors focus:border-moss"
            placeholder="Find risky behavior changes."
          />
        </label>

        <label className="space-y-2 text-sm font-semibold text-ink lg:col-span-2">
          <span>Body</span>
          <textarea
            value={form.body}
            onChange={(event) => setForm((current) => ({ ...current, body: event.target.value }))}
            className="min-h-40 w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 font-mono text-sm text-ink outline-none transition-colors focus:border-moss"
            placeholder="Write the reusable prompt or skill instructions."
          />
        </label>

        <label className="space-y-2 text-sm font-semibold text-ink">
          <span>Tags JSON</span>
          <textarea
            value={form.tags_json}
            onChange={(event) =>
              setForm((current) => ({ ...current, tags_json: event.target.value }))
            }
            className="min-h-24 w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 font-mono text-sm text-ink outline-none transition-colors focus:border-moss"
          />
        </label>

        <label className="space-y-2 text-sm font-semibold text-ink">
          <span>Metadata JSON</span>
          <textarea
            value={form.metadata_json}
            onChange={(event) =>
              setForm((current) => ({ ...current, metadata_json: event.target.value }))
            }
            className="min-h-24 w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 font-mono text-sm text-ink outline-none transition-colors focus:border-moss"
          />
        </label>

        <label className="flex items-center gap-2 text-sm font-semibold text-ink">
          <input
            type="checkbox"
            checked={form.enabled}
            onChange={(event) =>
              setForm((current) => ({ ...current, enabled: event.target.checked }))
            }
            className="h-4 w-4 rounded border-ink/20 text-moss focus:ring-moss"
          />
          <span>Enabled</span>
        </label>

        <div className="flex flex-wrap items-center gap-3 lg:col-span-2">
          <Button type="submit" disabled={createMutation.isPending}>
            {createMutation.isPending ? "Creating..." : "Create library item"}
          </Button>
          {formError && <p className="text-sm text-ember">{formError}</p>}
          {createMutation.error && !formError && (
            <p className="text-sm text-ember">Could not create library item.</p>
          )}
        </div>
      </form>

      {assets.length === 0 ? (
        <div className="rounded-3xl border border-dashed border-ink/20 bg-white/70 p-8 text-steel shadow-sm">
          No prompts or skills yet. Create one above.
        </div>
      ) : (
        <div className="grid gap-3">
          {assets.map((asset) => {
            const enabled = asset.enabled === 1;
            return (
              <article
                key={asset.id}
                className="rounded-3xl border border-ink/10 bg-white/75 p-5 shadow-sm"
              >
                <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
                  <div>
                    <p className="font-display text-lg font-semibold text-ink">{asset.name}</p>
                    <p className="text-sm text-steel">
                      {asset.item_type} - {asset.description ?? "No description"}
                    </p>
                  </div>
                  <span
                    className={`rounded-full px-3 py-1 text-xs font-semibold uppercase tracking-wide ${
                      enabled ? "bg-moss/10 text-moss" : "bg-ink/10 text-steel"
                    }`}
                  >
                    {enabled ? "enabled" : "disabled"}
                  </span>
                </div>

                <pre className="mt-4 overflow-x-auto rounded-2xl bg-paper/70 p-3 font-mono text-xs text-ink">
                  {asset.body}
                </pre>

                <div className="mt-4 grid gap-3 md:grid-cols-2">
                  <pre className="overflow-x-auto rounded-2xl bg-paper/70 p-3 font-mono text-xs text-ink">
                    {asset.tags_json}
                  </pre>
                  <pre className="overflow-x-auto rounded-2xl bg-paper/70 p-3 font-mono text-xs text-ink">
                    {asset.metadata_json}
                  </pre>
                </div>

                <div className="mt-4">
                  <Button
                    type="button"
                    variant="secondary"
                    aria-label={enabled ? `Disable ${asset.name}` : `Enable ${asset.name}`}
                    disabled={toggleMutation.isPending}
                    onClick={() =>
                      toggleMutation.mutate({
                        id: asset.id,
                        enabled: !enabled,
                      })
                    }
                  >
                    {enabled ? "Disable" : "Enable"}
                  </Button>
                </div>
              </article>
            );
          })}
        </div>
      )}
    </section>
  );
}
