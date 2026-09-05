import { useMutation, useQueryClient } from "@tanstack/react-query";
import { ImportPanel } from "../components/imports/ImportPanel";
import { DismissButton } from "../components/ui/DismissButton";
import { importExampleJson } from "../lib/api/client";

export function ImportsScreen() {
  const queryClient = useQueryClient();
  const importMutation = useMutation({
    mutationFn: importExampleJson,
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["batch-groups"] }),
  });

  return (
    <section className="space-y-3">
      <div className="rounded-2xl border border-stone-200 bg-white/82 px-4 py-3 shadow-sm">
        <p className="text-[11px] font-semibold uppercase tracking-wide text-stone-400">Data</p>
        <h1 className="mt-0.5 text-lg font-semibold text-stone-950">Imports</h1>
      </div>
      <ImportPanel onImport={(request) => importMutation.mutateAsync(request).then(() => undefined)} />
      {importMutation.data && (
        <div className="flex items-start justify-between gap-3 rounded-xl border border-emerald-100 bg-emerald-50 px-3 py-2 text-[13px] font-medium text-emerald-700">
          <p>
            Imported {importMutation.data.success_count} records into batch {importMutation.data.batch_id}.
          </p>
          <DismissButton ariaLabel="Dismiss import result" onClick={() => importMutation.reset()} />
        </div>
      )}
      {importMutation.error && (
        <div className="flex items-start justify-between gap-3 text-[13px] font-medium text-red-700">
          <p>Import failed.</p>
          <DismissButton ariaLabel="Dismiss import error" onClick={() => importMutation.reset()} />
        </div>
      )}
    </section>
  );
}
