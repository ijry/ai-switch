import type { SkillItem, SkillPackageDetail as SkillPackageDetailData } from "../../lib/api/types";
import { useI18n } from "../../lib/i18n";
import { skillPackageNameKey, skillSourceLabelKey } from "./catalog";

type SkillPackageDetailProps = {
  detail: SkillPackageDetailData | null;
  installing: boolean;
  loading: boolean;
  onInstallMissing: () => void;
  onSelectSkill: (skill: SkillItem) => void;
};

export function SkillPackageDetail({
  detail,
  installing,
  loading,
  onInstallMissing,
  onSelectSkill,
}: SkillPackageDetailProps) {
  const { t } = useI18n();

  if (loading) {
    return (
      <main className="grid min-h-[360px] min-w-0 place-items-center border border-stone-200 bg-white p-4 text-[12px] text-stone-500 shadow-sm">
        {t("skills.packageScanning")}
      </main>
    );
  }
  if (!detail) {
    return (
      <main className="grid min-h-[360px] min-w-0 place-items-center border border-stone-200 bg-white p-4 text-center text-[12px] text-stone-500 shadow-sm">
        {t("skills.packageSelect")}
      </main>
    );
  }

  const packageName = skillPackageNameKey(detail.package.id)
    ? t(skillPackageNameKey(detail.package.id)!)
    : detail.package.name;

  return (
    <main className="min-h-0 min-w-0 overflow-hidden border border-stone-200 bg-white p-4 shadow-sm">
      <div className="flex min-w-0 flex-wrap items-start justify-between gap-3 border-b border-stone-200 pb-3">
        <div className="min-w-0">
          <h2 className="truncate text-[15px] font-semibold text-stone-950">{packageName}</h2>
          <p className="mt-1 break-all font-mono text-[11px] text-stone-500">{detail.package.id}</p>
        </div>
        <button
          className="shrink-0 bg-stone-900 px-3 py-2 text-[12px] font-semibold text-white hover:bg-stone-800 disabled:cursor-not-allowed disabled:opacity-50"
          disabled={installing || detail.package.installed_count >= detail.package.skill_count}
          onClick={onInstallMissing}
          type="button"
        >
          {installing ? t("skills.packageInstalling") : t("skills.packageInstall")}
        </button>
      </div>
      <dl className="grid gap-x-4 gap-y-2 border-b border-stone-100 py-3 text-[11px] sm:grid-cols-2">
        <div>
          <dt className="font-semibold text-stone-500">{t("skills.packageVersion")}</dt>
          <dd className="mt-0.5 text-stone-800">{detail.package.version ?? "—"}</dd>
        </div>
        <div>
          <dt className="font-semibold text-stone-500">{t("skills.packageInstallProgress")}</dt>
          <dd className="mt-0.5 break-all text-stone-800">
            {t("skills.packageInstalledCount", {
              installed: detail.package.installed_count,
              total: detail.package.skill_count,
            })}
          </dd>
        </div>
        <div>
          <dt className="font-semibold text-stone-500">{t("skills.packageSource")}</dt>
          <dd className="mt-0.5 text-stone-800">{t(skillSourceLabelKey(detail.package.source))}</dd>
        </div>
        <div className="min-w-0">
          <dt className="font-semibold text-stone-500">{t("skills.packageMembers")}</dt>
          <dd className="mt-0.5 break-all font-mono text-stone-800">
            {t("skills.packageCount", { count: detail.package.skill_count })}
          </dd>
        </div>
      </dl>
      <div className="min-h-0 overflow-y-auto pt-3">
        <h3 className="text-[11px] font-semibold uppercase tracking-wide text-stone-400">
          {t("skills.packageMembers")}
        </h3>
        <div className="mt-2 grid gap-1 sm:grid-cols-2">
          {(detail.members.length
            ? detail.members
            : detail.skills.map((skill) => ({
                id: skill.id,
                name: skill.name,
                description: skill.description,
                category: skill.category ?? null,
                tags: skill.tags ?? [],
                language: skill.language ?? null,
                installed: true,
                skill,
              }))
          ).map((member) => (
            <button
              className="min-w-0 border border-stone-200 px-2.5 py-2 text-left hover:border-stone-400 hover:bg-stone-50"
              disabled={!member.skill}
              key={member.id}
              onClick={() => {
                if (member.skill) onSelectSkill(member.skill);
              }}
              type="button"
            >
              <span className="flex min-w-0 items-center justify-between gap-2">
                <span className="truncate text-[12px] font-semibold text-stone-900">{member.name}</span>
                <span className={`shrink-0 text-[10px] ${member.installed ? "text-emerald-700" : "text-stone-400"}`}>
                  {member.installed ? t("skills.packageMemberInstalled") : t("skills.packageMemberMissing")}
                </span>
              </span>
              <span className="mt-0.5 block truncate font-mono text-[10px] text-stone-500">{member.id}</span>
            </button>
          ))}
        </div>
      </div>
    </main>
  );
}
