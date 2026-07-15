import { useEffect, useRef, useState, type ChangeEvent } from "react";
import { ScanText } from "lucide-react";
import { recognizeImageText } from "../lib/ocr/recognizeText";
import { useI18n } from "../lib/i18n";

type OcrError = "no-file" | "invalid-file" | "failed" | null;

const errorKeys = {
  "no-file": "ocr.error.noFile",
  "invalid-file": "ocr.error.invalidFile",
  failed: "ocr.error.failed",
} as const;

export function OcrScreen() {
  const { t } = useI18n();
  const imageRef = useRef<HTMLImageElement | null>(null);
  const [fileName, setFileName] = useState<string | null>(null);
  const [previewUrl, setPreviewUrl] = useState<string | null>(null);
  const [result, setResult] = useState("");
  const [error, setError] = useState<OcrError>(null);
  const [recognizing, setRecognizing] = useState(false);

  useEffect(() => {
    return () => {
      if (previewUrl && typeof URL.revokeObjectURL === "function") {
        URL.revokeObjectURL(previewUrl);
      }
    };
  }, [previewUrl]);

  const handleFileChange = (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    setResult("");
    if (!file) {
      setFileName(null);
      setPreviewUrl(null);
      setError(null);
      return;
    }
    if (!file.type.startsWith("image/")) {
      setFileName(null);
      setPreviewUrl(null);
      setError("invalid-file");
      return;
    }
    const nextUrl = URL.createObjectURL(file);
    if (previewUrl && typeof URL.revokeObjectURL === "function") {
      URL.revokeObjectURL(previewUrl);
    }
    setFileName(file.name);
    setPreviewUrl(nextUrl);
    setError(null);
  };

  const runRecognition = async () => {
    if (!imageRef.current) {
      setError("no-file");
      return;
    }
    setRecognizing(true);
    setError(null);
    try {
      setResult(await recognizeImageText(imageRef.current));
    } catch {
      setError("failed");
    } finally {
      setRecognizing(false);
    }
  };

  return (
    <section className="space-y-3">
      <div className="rounded-2xl border border-stone-200 bg-white/82 p-4 shadow-sm">
        <p className="text-[11px] font-semibold uppercase tracking-wide text-stone-400">{t("ocr.kicker")}</p>
        <h1 className="mt-0.5 text-lg font-semibold tracking-tight text-stone-950">{t("ocr.title")}</h1>
        <p className="mt-1 text-[13px] text-stone-500">{t("ocr.subtitle")}</p>
        <p className="mt-2 rounded-xl border border-amber-200 bg-amber-50 px-3 py-2 text-[12px] font-medium text-amber-800">
          {t("ocr.limit")}
        </p>
      </div>

      <div className="grid gap-3 lg:grid-cols-[minmax(0,1fr)_360px]">
        <div className="rounded-2xl border border-stone-200 bg-white/82 p-4 shadow-sm">
          <label className="flex flex-col gap-1.5 text-[12px] font-semibold text-stone-600">
            <span>{t("ocr.file")}</span>
            <input
              accept="image/*"
              aria-label={t("ocr.file")}
              className="rounded-xl border border-stone-200 bg-white px-3 py-2 text-[13px] font-medium text-stone-900 file:mr-3 file:rounded-lg file:border-0 file:bg-stone-900 file:px-3 file:py-1.5 file:text-[12px] file:font-semibold file:text-white"
              onChange={handleFileChange}
              type="file"
            />
          </label>
          <p className="mt-2 text-[12px] text-stone-500">{t("ocr.fileHint")}</p>
          <p className="mt-3 text-[13px] font-medium text-stone-700">{fileName ?? t("ocr.noFile")}</p>
          {error && (
            <p className="mt-2 text-[13px] font-medium text-red-700">
              {t(errorKeys[error] as Parameters<typeof t>[0])}
            </p>
          )}
          <button
            className="mt-4 inline-flex items-center gap-2 rounded-xl bg-stone-900 px-3 py-2 text-[13px] font-semibold text-white transition-colors hover:bg-stone-800 disabled:opacity-50"
            disabled={recognizing}
            onClick={runRecognition}
            type="button"
          >
            <ScanText className="h-4 w-4" />
            {recognizing ? t("ocr.recognizing") : t("ocr.recognize")}
          </button>
        </div>

        <div className="rounded-2xl border border-stone-200 bg-white/82 p-4 shadow-sm">
          {previewUrl ? (
            <img
              alt={fileName ?? t("ocr.file")}
              className="max-h-80 w-full rounded-xl border border-stone-200 object-contain"
              ref={imageRef}
              src={previewUrl}
            />
          ) : (
            <div className="grid min-h-60 place-items-center rounded-xl border border-dashed border-stone-300 bg-stone-50 text-[13px] text-stone-500">
              {t("ocr.noFile")}
            </div>
          )}
        </div>
      </div>

      <label className="flex flex-col gap-1.5 rounded-2xl border border-stone-200 bg-white/82 p-4 shadow-sm">
        <span className="text-[12px] font-semibold text-stone-600">{t("ocr.result")}</span>
        <textarea
          aria-label={t("ocr.result")}
          className="min-h-48 resize-y rounded-xl border border-stone-200 bg-stone-50 px-3 py-2 text-[13px] text-stone-900 outline-none"
          placeholder={t("ocr.resultPlaceholder")}
          readOnly
          value={result}
        />
      </label>
    </section>
  );
}
