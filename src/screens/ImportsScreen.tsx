import { useMutation, useQueryClient } from "@tanstack/react-query";
import { ImportPanel } from "../components/imports/ImportPanel";
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
        <p className="rounded-xl border border-emerald-100 bg-emerald-50 px-3 py-2 text-[13px] font-medium text-emerald-700">
          Imported {importMutation.data.success_count} records into batch {importMutation.data.batch_id}.
        </p>
      )}
      {importMutation.error && <p className="text-[13px] font-medium text-red-700">Import failed.</p>}
    </section>
  );
}
