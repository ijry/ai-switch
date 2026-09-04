import { motion } from "motion/react";
export type FormTab<T extends string> = {
  value: T;
  label: string;
};

export function FormTabs<T extends string>({
  ariaLabel,
  onChange,
  tabs,
  value,
}: {
  ariaLabel: string;
  onChange: (value: T) => void;
  tabs: Array<FormTab<T>>;
  value: T;
}) {
  return (
    <div aria-label={ariaLabel} className="flex border-b border-stone-200" role="tablist">
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
              layoutId={`form-tab-indicator-${ariaLabel}`}
              transition={{ duration: 0.2, ease: [0.22, 1, 0.36, 1] }}
            />
          ) : null}
        </button>
      ))}
    </div>
  );
}
