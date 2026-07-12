import { useState } from "react";
import { Button } from "../ui/Button";

type ImportRequest = {
  batch_name: string;
  source_label: string;
  strategy: string;
  json: string;
};

type ImportPanelProps = {
  onImport: (request: ImportRequest) => Promise<void>;
};

export function ImportPanel({ onImport }: ImportPanelProps) {
  const [batchName, setBatchName] = useState("");
  const [sourceLabel, setSourceLabel] = useState("manual paste");
  const [json, setJson] = useState("{\"providers\":[],\"accounts\":[]}");
  const [error, setError] = useState<string | null>(null);

  async function submit() {
    if (batchName.trim().length === 0) {
      setError("Batch name is required.");
      return;
    }

    setError(null);
    await onImport({
      batch_name: batchName.trim(),
      source_label: sourceLabel.trim() || "manual paste",
      strategy: "skip",
      json,
    });
  }

  return (
    <div className="space-y-4 rounded-3xl border border-ink/10 bg-white/75 p-5 shadow-sm shadow-ink/5">
      <label className="block text-sm font-semibold text-ink">
        Batch name
        <input
          value={batchName}
          onChange={(event) => setBatchName(event.target.value)}
          className="mt-2 w-full rounded-2xl border border-ink/10 px-4 py-3 outline-none focus:border-moss focus:ring-2 focus:ring-moss/20"
        />
      </label>
      <label className="block text-sm font-semibold text-ink">
        Source label
        <input
          value={sourceLabel}
          onChange={(event) => setSourceLabel(event.target.value)}
          className="mt-2 w-full rounded-2xl border border-ink/10 px-4 py-3 outline-none focus:border-moss focus:ring-2 focus:ring-moss/20"
        />
      </label>
      <label className="block text-sm font-semibold text-ink">
        JSON
        <textarea
          value={json}
          onChange={(event) => setJson(event.target.value)}
          rows={8}
          className="mt-2 w-full rounded-2xl border border-ink/10 px-4 py-3 font-mono text-sm outline-none focus:border-moss focus:ring-2 focus:ring-moss/20"
        />
      </label>
      {error && <p className="text-sm font-medium text-ember">{error}</p>}
      <Button type="button" onClick={submit}>
        Import
      </Button>
    </div>
  );
}
