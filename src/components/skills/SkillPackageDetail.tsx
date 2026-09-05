import { Trash2 } from "lucide-react";
import type {
  SkillItem,
  SkillPackageDetail as SkillPackageDetailData,
  SkillPackageMember,
} from "../../lib/api/types";
import { useI18n } from "../../lib/i18n";
import { skillDisplayCopy, skillPackageNameKey, skillPackageSummaryKey, skillSourceLabelKey } from "./catalog";

type SkillPackageDetailProps = {
  detail: SkillPackageDetailData | null;
  installing: boolean;
  uninstalling: boolean;
  busySkillId: string | null;
  loading: boolean;
  onInstallMissing: () => void;
  onUninstallAll: () => void;
  onInstallMember: (skillId: string) => void;
  onUninstallMember: (skillId: string) => void;
  onSelectSkill: (skill: SkillItem) => void;
};

/// The pack ships every member as a directory, so a member with no `skill` is one
/// that is simply not installed yet — the row still has to render, with an install
/// button instead of a link into the editor.
function membersOf(detail: SkillPackageDetailData): SkillPackageMember[] {
  if (detail.members.length) return detail.members;
  return detail.skills.map((skill) => ({
    id: skill.id,
    name: skill.name,
    description: skill.description,
    category: skill.category ?? null,
    tags: skill.tags ?? [],
    language: skill.language ?? null,
    installed: true,
    skill,
  }));
}

export function SkillPackageDetail({
  detail,
  installing,
  uninstalling,
  busySkillId,
  loading,
  onInstallMissing,
  onUninstallAll,
  onInstallMember,
  onUninstallMember,
  onSelectSkill,
}: SkillPackageDetailProps) {
  const { t } = useI18n();

  if (loading) {
    return (
      <main className="grid min-h-[360px] min-w-0 place-items-center rounded-2xl border border-stone-200 bg-white p-4 text-[12px] text-stone-500 shadow-sm">
        {t("skills.packageScanning")}
      </main>
    );
  }
  if (!detail) {
    return (
      <main className="grid min-h-[360px] min-w-0 place-items-center rounded-2xl border border-stone-200 bg-white p-4 text-center text-[12px] text-stone-500 shadow-sm">
        {t("skills.packageSelect")}
      </main>
    );
  }

  const nameKey = skillPackageNameKey(detail.package.id);
  const packageName = nameKey ? t(nameKey) : detail.package.name;
  const summaryKey = skillPackageSummaryKey(detail.package.id);
  const packageSummary = summaryKey ? t(summaryKey) : detail.package.description;
  const members = membersOf(detail);
  const installedCount = members.filter((member) => member.installed).length;
  const busy = installing || uninstalling || busySkillId !== null;

  return (
    <main className="flex min-h-0 min-w-0 flex-col overflow-hidden rounded-2xl border border-stone-200 bg-white p-4 shadow-sm">
      <div className="flex min-w-0 flex-wrap items-start justify-between gap-3 border-b border-stone-200 pb-3">
        <div className="min-w-0">
          <h2 className="truncate text-[15px] font-semibold text-stone-950">{packageName}</h2>
          <p className="mt-1 break-all font-mono text-[11px] text-stone-500">{detail.package.id}</p>
          {packageSummary ? <p className="mt-1.5 text-[12px] text-stone-600">{packageSummary}</p> : null}
        </div>
        {/* Not `shrink-0`: a group that cannot shrink is sized to its own content,
            so `flex-wrap` would never fire and the second button would spill out of
            the card when the pane is narrow. */}
        <div className="flex flex-wrap items-center gap-2">
          <button
            className="shrink-0 whitespace-nowrap rounded-xl bg-stone-900 px-3 py-2 text-[12px] font-semibold text-white motion-control hover:bg-stone-800 disabled:cursor-not-allowed disabled:opacity-50"
            disabled={busy || installedCount >= members.length}
            onClick={onInstallMissing}
            type="button"
          >
            {installing ? t("skills.packageInstalling") : t("skills.packageInstall")}
          </button>
          <button
            className="inline-flex shrink-0 items-center gap-1.5 whitespace-nowrap rounded-xl bg-white px-3 py-2 text-[12px] font-semibold text-red-700 ring-1 ring-red-200 motion-control hover:bg-red-50 disabled:cursor-not-allowed disabled:opacity-50"
            disabled={busy || installedCount === 0}
            onClick={onUninstallAll}
            type="button"
          >
            <Trash2 className="h-3.5 w-3.5" />
            {uninstalling ? t("skills.packageUninstalling") : t("skills.packageUninstall")}
          </button>
        </div>
      </div>
      <dl className="flex flex-wrap gap-x-6 gap-y-2 border-b border-stone-100 py-3 text-[11px]">
        <div className="min-w-0">
          <dt className="font-semibold text-stone-500">{t("skills.packageVersion")}</dt>
          <dd className="mt-0.5 text-stone-800">{detail.package.version ?? "—"}</dd>
        </div>
        <div className="min-w-0">
          <dt className="font-semibold text-stone-500">{t("skills.packageInstallProgress")}</dt>
          <dd className="mt-0.5 whitespace-nowrap text-stone-800">
            {t("skills.packageInstalledCount", { installed: installedCount, total: members.length })}
          </dd>
        </div>
        <div className="min-w-0">
          <dt className="font-semibold text-stone-500">{t("skills.packageSource")}</dt>
          <dd className="mt-0.5 whitespace-nowrap text-stone-800">{t(skillSourceLabelKey(detail.package.source))}</dd>
        </div>
      </dl>
      <div className="min-h-0 flex-1 overflow-y-auto pt-3">
        <h3 className="text-[11px] font-semibold uppercase tracking-wide text-stone-400">
          {t("skills.packageMembers")}
        </h3>
        <div className="mt-2 grid gap-1">
          {members.map((member) => (
            <MemberRow
              busy={busy}
              busySkillId={busySkillId}
              key={member.id}
              member={member}
              onInstall={onInstallMember}
              onSelectSkill={onSelectSkill}
              onUninstall={onUninstallMember}
            />
          ))}
        </div>
      </div>
    </main>
  );
}

function MemberRow({
  busy,
  busySkillId,
  member,
  onInstall,
  onSelectSkill,
  onUninstall,
}: {
  busy: boolean;
  busySkillId: string | null;
  member: SkillPackageMember;
  onInstall: (skillId: string) => void;
  onSelectSkill: (skill: SkillItem) => void;
  onUninstall: (skillId: string) => void;
}) {
  const { language, t } = useI18n();
  const copy = skillDisplayCopy(member, language);
  const readOnly = member.skill?.read_only ?? false;
  const pending = busySkillId === member.id;

  return (
    <div className="flex min-w-0 flex-wrap items-center gap-2 rounded-xl bg-white px-2.5 py-2 ring-1 ring-stone-200">
      <button
        className="min-w-[8rem] flex-1 rounded-lg text-left motion-control hover:bg-stone-50 disabled:cursor-default"
        disabled={!member.skill}
        onClick={() => {
          if (member.skill) onSelectSkill(member.skill);
        }}
        type="button"
      >
        <span className="block truncate text-[12px] font-semibold text-stone-900">{copy.name}</span>
        <span className="mt-0.5 block truncate font-mono text-[10px] text-stone-500">{member.id}</span>
        {copy.description ? (
          <span className="mt-0.5 line-clamp-2 block text-[11px] text-stone-500">{copy.description}</span>
        ) : null}
      </button>
      {/* `ml-auto` keeps the actions right-aligned on the row's second line once the
          name column claims its 8rem and the group wraps. */}
      <div className="ml-auto flex shrink-0 items-center gap-2">
        <span className={`whitespace-nowrap text-[10px] ${member.installed ? "text-emerald-700" : "text-stone-400"}`}>
          {member.installed ? t("skills.packageMemberInstalled") : t("skills.packageMemberMissing")}
        </span>
        {member.installed ? (
          <button
            className="whitespace-nowrap rounded-lg bg-white px-2 py-1 text-[11px] font-semibold text-red-700 ring-1 ring-red-200 motion-control hover:bg-red-50 disabled:cursor-not-allowed disabled:opacity-40"
            disabled={busy || readOnly}
            onClick={() => onUninstall(member.id)}
            title={readOnly ? t("errors.skills.readOnly") : t("skills.packageUninstallMember")}
            type="button"
          >
            {pending ? t("skills.packageUninstalling") : t("skills.packageUninstallMember")}
          </button>
        ) : (
          <button
            className="whitespace-nowrap rounded-lg bg-stone-900 px-2 py-1 text-[11px] font-semibold text-white motion-control hover:bg-stone-800 disabled:cursor-not-allowed disabled:opacity-40"
            disabled={busy}
            onClick={() => onInstall(member.id)}
            title={t("skills.packageInstallMember")}
            type="button"
          >
            {pending ? t("skills.packageInstalling") : t("skills.packageInstallMember")}
          </button>
        )}
      </div>
    </div>
  );
}
