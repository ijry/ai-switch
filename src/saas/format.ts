import type { SaasLocale } from "./types";

export function decimalToInteger(value: string, decimals = 6): number {
  const clean = value.trim();
  if (!new RegExp(`^\\d+(?:\\.\\d{1,${decimals}})?$`).test(clean)) throw new Error(`Use a non-negative number with at most ${decimals} decimal places.`);
  const [whole, fraction = ""] = clean.split(".");
  const result = BigInt(whole) * 10n ** BigInt(decimals) + BigInt(fraction.padEnd(decimals, "0"));
  if (result > BigInt(Number.MAX_SAFE_INTEGER)) throw new Error("The amount is too large.");
  return Number(result);
}

export function integerToDecimal(value: number | null | undefined, decimals = 6): string {
  if (value == null || !Number.isSafeInteger(value)) return "";
  const sign = value < 0 ? "-" : "";
  const digits = Math.abs(value).toString().padStart(decimals + 1, "0");
  const fraction = digits.slice(-decimals).replace(/0+$/, "");
  return `${sign}${digits.slice(0, -decimals)}${fraction ? `.${fraction}` : ""}`;
}

export function formatMoney(value: number | null | undefined, locale: SaasLocale, currency: "USD" | "CNY" = "USD"): string {
  if (value == null || !Number.isSafeInteger(value)) return "—";
  return new Intl.NumberFormat(locale === "zh" ? "zh-CN" : "en-US", {
    style: "currency", currency, currencyDisplay: "narrowSymbol", minimumFractionDigits: 2, maximumFractionDigits: currency === "USD" ? 6 : 2,
  }).format(value / (currency === "USD" ? 1000000 : 100));
}

export function formatCount(value: number | undefined, locale: SaasLocale): string {
  return value == null || !Number.isFinite(value) ? "—" : new Intl.NumberFormat(locale === "zh" ? "zh-CN" : "en-US").format(value);
}

export function formatDate(value: string | null | undefined, locale: SaasLocale): string {
  if (!value || Number.isNaN(Date.parse(value))) return "—";
  return new Intl.DateTimeFormat(locale === "zh" ? "zh-CN" : "en-US", { dateStyle: "medium", timeStyle: "short" }).format(new Date(value));
}

export function estimateRecharge(amountCnyFen: number, exchangeRateMicros: number | null | undefined): number | undefined {
  if (!Number.isSafeInteger(amountCnyFen) || exchangeRateMicros == null || !Number.isSafeInteger(exchangeRateMicros) || amountCnyFen < 0 || exchangeRateMicros <= 0) return undefined;
  const amount = BigInt(amountCnyFen) * 10000000000n / BigInt(exchangeRateMicros);
  return amount <= BigInt(Number.MAX_SAFE_INTEGER) ? Number(amount) : undefined;
}

export function downloadText(filename: string, content: string) {
  const url = URL.createObjectURL(new Blob([content], { type: "text/plain;charset=utf-8" }));
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}
