import { useMemo, useState } from "react";
import { Copy } from "lucide-react";
import { transformCryptoText, type CryptoOperation } from "../lib/cryptoTransforms";
import { useI18n } from "../lib/i18n";

const operations: Array<{ value: CryptoOperation; labelKey: string }> = [
  { value: "base64-encode", labelKey: "crypto.base64Encode" },
  { value: "base64-decode", labelKey: "crypto.base64Decode" },
  { value: "url-encode", labelKey: "crypto.urlEncode" },
  { value: "url-decode", labelKey: "crypto.urlDecode" },
  { value: "hex-encode", labelKey: "crypto.hexEncode" },
  { value: "hex-decode", labelKey: "crypto.hexDecode" },
];

const errorKeys = {
  "invalid-base64": "crypto.error.invalidBase64",
  "invalid-url": "crypto.error.invalidUrl",
  "invalid-hex-length": "crypto.error.invalidHexLength",
  "invalid-hex": "crypto.error.invalidHex",
} as const;

export function CryptoToolsScreen() {
  const { t } = useI18n();
  const [input, setInput] = useState("");
  const [operation, setOperation] = useState<CryptoOperation>("base64-encode");
  const [copied, setCopied] = useState(false);
  const result = useMemo(() => transformCryptoText(input, operation), [input, operation]);

  const copyOutput = async () => {
    if (!result.output || typeof navigator === "undefined" || !navigator.clipboard) {
      return;
    }
    await navigator.clipboard.writeText(result.output);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1400);
  };

  return (
    <section className="space-y-3">
      <div className="rounded-2xl border border-stone-200 bg-white/82 p-4 shadow-sm">
        <p className="text-[11px] font-semibold uppercase tracking-wide text-stone-400">{t("crypto.kicker")}</p>
        <h1 className="mt-0.5 text-lg font-semibold tracking-tight text-stone-950">{t("crypto.title")}</h1>
        <p className="mt-1 text-[13px] text-stone-500">{t("crypto.subtitle")}</p>
      </div>

      <div className="grid gap-3 lg:grid-cols-[minmax(0,1fr)_260px]">
        <label className="flex flex-col gap-1.5 rounded-2xl border border-stone-200 bg-white/82 p-4 shadow-sm">
          <span className="text-[12px] font-semibold text-stone-600">{t("crypto.input")}</span>
          <textarea
            aria-label={t("crypto.input")}
            className="min-h-64 resize-y rounded-xl border border-stone-200 bg-stone-50 px-3 py-2 text-[13px] text-stone-900 outline-none transition focus:border-blue-400 focus:ring-2 focus:ring-blue-100"
            onChange={(event) => setInput(event.target.value)}
            placeholder={t("crypto.inputPlaceholder")}
            value={input}
          />
        </label>

        <label className="flex h-fit flex-col gap-1.5 rounded-2xl border border-stone-200 bg-white/82 p-4 shadow-sm">
          <span className="text-[12px] font-semibold text-stone-600">{t("crypto.operation")}</span>
          <select
            aria-label={t("crypto.operation")}
            className="rounded-xl border border-stone-200 bg-white px-3 py-2 text-[13px] font-medium text-stone-900 outline-none transition focus:border-blue-400 focus:ring-2 focus:ring-blue-100"
            onChange={(event) => setOperation(event.target.value as CryptoOperation)}
            value={operation}
          >
            {operations.map((item) => (
              <option key={item.value} value={item.value}>
                {t(item.labelKey as Parameters<typeof t>[0])}
              </option>
            ))}
          </select>
        </label>
      </div>

      <div className="rounded-2xl border border-stone-200 bg-white/82 p-4 shadow-sm">
        <div className="mb-2 flex items-center justify-between gap-2">
          <label className="text-[12px] font-semibold text-stone-600" htmlFor="crypto-output">
            {t("crypto.output")}
          </label>
          <button
            className="inline-flex items-center gap-1.5 rounded-xl border border-stone-200 bg-white px-3 py-2 text-[12px] font-semibold text-stone-700 transition-colors hover:bg-stone-50 disabled:opacity-50"
            disabled={!result.output}
            onClick={copyOutput}
            type="button"
          >
            <Copy className="h-3.5 w-3.5" />
            {copied ? t("crypto.copied") : t("crypto.copy")}
          </button>
        </div>
        <textarea
          aria-label={t("crypto.output")}
          className="min-h-48 w-full resize-y rounded-xl border border-stone-200 bg-stone-50 px-3 py-2 text-[13px] text-stone-900 outline-none"
          id="crypto-output"
          readOnly
          value={result.output}
        />
        {result.error && (
          <p className="mt-2 text-[13px] font-medium text-red-700">{t(errorKeys[result.error])}</p>
        )}
      </div>
    </section>
  );
}
