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

  const fieldClass =
    "mt-1.5 w-full rounded-xl border border-stone-200 bg-white px-3 py-2 text-[13px] text-stone-900 outline-none transition focus:border-blue-400 focus:ring-2 focus:ring-blue-100";
  const labelClass = "block text-[12px] font-semibold text-stone-600";

  return (
    <div className="space-y-3 rounded-2xl border border-stone-200 bg-white/82 p-4 shadow-sm">
      <label className={labelClass}>
        Batch name
        <input
          value={batchName}
          onChange={(event) => setBatchName(event.target.value)}
          className={fieldClass}
        />
      </label>
      <label className={labelClass}>
        Source label
        <input
          value={sourceLabel}
          onChange={(event) => setSourceLabel(event.target.value)}
          className={fieldClass}
        />
      </label>
      <label className={labelClass}>
        JSON
        <textarea
          value={json}
          onChange={(event) => setJson(event.target.value)}
          rows={8}
          className={`${fieldClass} font-mono`}
        />
      </label>
      {error && <p className="text-[13px] font-medium text-red-700">{error}</p>}
      <Button type="button" onClick={submit}>
        Import
      </Button>
    </div>
  );
}
