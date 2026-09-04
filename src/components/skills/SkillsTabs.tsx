import { motion } from "motion/react";
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
          className={`relative border-b-2 px-3 py-2 text-[12px] font-semibold motion-control ${
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
          {value === tab.value ? (
            <motion.span
              aria-hidden="true"
              className="absolute inset-x-0 bottom-[-2px] h-0.5 bg-stone-900"
              layoutId="skills-tab-indicator"
              transition={{ duration: 0.2, ease: [0.22, 1, 0.36, 1] }}
            />
          ) : null}
        </button>
      ))}
    </div>
  );
}
