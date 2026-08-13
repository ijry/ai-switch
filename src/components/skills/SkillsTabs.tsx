import { useI18n } from "../../lib/i18n";

export type SkillsView = "skills" | "packages";

export function SkillsTabs({
  value,
  onChange,
}: {
  value: SkillsView;
  onChange: (value: SkillsView) => void;
}) {
  const { t } = useI18n();
  const tabs: Array<{ value: SkillsView; label: string }> = [
    { value: "skills", label: t("skills.tabSkills") },
    { value: "packages", label: t("skills.tabPackages") },
  ];

  return (
    <div aria-label={t("skills.title")} className="flex border-b border-stone-200" role="tablist">
      {tabs.map((tab) => (
        <button
          aria-selected={value === tab.value}
          className={`border-b-2 px-3 py-2 text-[12px] font-semibold ${
            value === tab.value
              ? "border-stone-900 text-stone-950"
              : "border-transparent text-stone-500 hover:border-stone-300 hover:text-stone-800"
          }`}
          key={tab.value}
          onClick={() => onChange(tab.value)}
          role="tab"
          type="button"
        >
          {tab.label}
        </button>
      ))}
    </div>
  );
}
