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
          className={`border-b-2 px-3 py-2 text-[12px] font-semibold transition-colors ${
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
