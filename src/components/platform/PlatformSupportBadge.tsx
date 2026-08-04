import type { PlatformSupportLevel } from "../../lib/api/types";

type PlatformSupportBadgeProps = {
  displayName: string;
  supportLevel: PlatformSupportLevel;
};

export function PlatformSupportBadge({
  displayName,
  supportLevel,
}: PlatformSupportBadgeProps) {
  const partial = supportLevel === "partial";
  const label = partial ? "部分支持" : "已支持";
  return (
    <span
      aria-label={`${displayName} ${label}`}
      className={`inline-flex rounded-full border px-2 py-0.5 text-[10px] font-semibold ${
        partial
          ? "border-amber-200 bg-amber-50 text-amber-800"
          : "border-emerald-200 bg-emerald-50 text-emerald-800"
      }`}
    >
      {label}
    </span>
  );
}
