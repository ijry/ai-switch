const TIERS = [
  { threshold: 100_000_000, suffix: "亿", decimals: 2 },
  { threshold: 1_000_000, suffix: "百万", decimals: 2 },
  { threshold: 10_000, suffix: "万", decimals: 1 },
] as const;

function sanitize(value: number) {
  return Number.isFinite(value) && value > 0 ? value : 0;
}

/**
 * Compact a count for a summary card or a table cell.
 *
 * The tier is chosen from the raw value before any rounding, so 99,999,999
 * stays in 百万 (as "100.00百万") instead of rounding itself up into 亿.
 */
export function formatCompactCount(value: number): string {
  const safe = sanitize(value);
  const tier = TIERS.find((candidate) => safe >= candidate.threshold);
  if (!tier) {
    return safe.toLocaleString("en-US");
  }
  return `${(safe / tier.threshold).toFixed(tier.decimals)}${tier.suffix}`;
}

/** The precise figure, for the `title` tooltip beside a compact value. */
export function formatExactCount(value: number): string {
  return sanitize(value).toLocaleString("en-US");
}
