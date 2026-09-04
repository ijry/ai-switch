const UNITS = ["B", "KB", "MB", "GB", "TB"] as const;

/**
 * Render a byte count for a human, in binary units.
 *
 * Disk figures span orders of magnitude — a 640 MB shortfall on a 2 TB volume —
 * so the unit follows the value instead of being fixed. Whole numbers keep no
 * decimal, so a 1 GiB threshold reads "1 GB" rather than "1.0 GB".
 */
export function formatByteSize(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) {
    return "0 B";
  }

  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }

  if (unit === 0) {
    return `${Math.round(value)} ${UNITS[unit]}`;
  }

  // Rounding can carry past the unit — 1023.97 MB is "1 GB", not "1024 MB".
  const rounded = Number(value.toFixed(1));
  if (rounded >= 1024 && unit < UNITS.length - 1) {
    return `${Number((rounded / 1024).toFixed(1))} ${UNITS[unit + 1]}`;
  }
  return `${rounded} ${UNITS[unit]}`;
}
