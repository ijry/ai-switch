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
    <section className="space-y-4">
      <div>
        <h1 className="font-display text-3xl font-semibold text-ink">Imports</h1>
        <p className="text-steel">Paste example JSON and assign it to a named batch.</p>
      </div>
      <ImportPanel onImport={(request) => importMutation.mutateAsync(request).then(() => undefined)} />
      {importMutation.data && (
        <p className="rounded-2xl bg-moss/10 p-4 text-moss">
          Imported {importMutation.data.success_count} records into batch {importMutation.data.batch_id}.
        </p>
      )}
      {importMutation.error && <p className="text-ember">Import failed.</p>}
    </section>
  );
}
