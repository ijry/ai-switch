import type { SkillPackage, SkillScanWarning } from "../../lib/api/types";
import { useI18n } from "../../lib/i18n";
import { skillPackageNameKey, skillSourceLabelKey } from "./catalog";

type SkillPackagesListProps = {
  packages: SkillPackage[];
  selectedId: string | null;
  loading: boolean;
  warnings: SkillScanWarning[];
  onSelect: (packageId: string) => void;
};

export function SkillPackagesList({
  packages,
  selectedId,
  loading,
  warnings,
  onSelect,
}: SkillPackagesListProps) {
  const { t } = useI18n();

  return (
    <aside className="flex min-h-0 min-w-0 flex-col overflow-hidden border border-stone-200 bg-white p-2 shadow-sm">
      <div className="flex items-center justify-between px-2 py-1">
        <p className="text-[11px] font-semibold uppercase tracking-wide text-stone-400">
          {t("skills.tabPackages")}
        </p>
        <span className="font-mono text-[11px] text-stone-400">{packages.length}</span>
      </div>
      {loading ? (
        <p className="px-2 py-5 text-[12px] text-stone-500">{t("skills.packageScanning")}</p>
      ) : packages.length ? (
        <div className="mt-1 min-h-0 flex-1 space-y-0.5 overflow-y-auto">
          {packages.map((item) => (
            <button
              aria-current={item.id === selectedId ? "true" : undefined}
              className={`w-full border px-2.5 py-2 text-left ${
                item.id === selectedId
                  ? "border-stone-300 bg-stone-100"
                  : "border-transparent hover:border-stone-200 hover:bg-stone-50"
              }`}
              key={item.id}
              onClick={() => onSelect(item.id)}
              type="button"
            >
              <div className="truncate text-[12px] font-semibold text-stone-900">
                {skillPackageNameKey(item.id) ? t(skillPackageNameKey(item.id)!) : item.name}
              </div>
              <div className="mt-1 flex items-center justify-between gap-2 text-[10px] text-stone-500">
                <span className="truncate">{t(skillSourceLabelKey(item.source))}</span>
                <span className="shrink-0">
                  {t("skills.packageInstalledCount", {
                    installed: item.installed_count,
                    total: item.skill_count,
                  })}
                </span>
              </div>
            </button>
          ))}
        </div>
      ) : (
        <p className="px-2 py-5 text-[12px] text-stone-500">{t("skills.packageEmpty")}</p>
      )}
      {warnings.length ? (
        <p className="mt-3 shrink-0 border-t border-amber-100 px-2 pt-2 text-[11px] text-amber-800">
          {t("skills.packageWarning")}
        </p>
      ) : null}
    </aside>
  );
}
