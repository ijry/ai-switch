import type { SkillItem, SkillLocation } from "../../lib/api/types";
import { useI18n } from "../../lib/i18n";

type SkillsListProps = {
  items: SkillItem[];
  total: number;
  filterText: string;
  loading: boolean;
  projectMissing: boolean;
  locations: SkillLocation[];
  selectedId: string | null;
  onSelect: (item: SkillItem) => void;
};

export function SkillsList({ items, total, filterText, loading, projectMissing, locations, selectedId, onSelect }: SkillsListProps) {
  const { t } = useI18n();

  return (
    <aside className="flex min-h-0 min-w-0 flex-col overflow-hidden border border-stone-200 bg-white p-2 shadow-sm">
      <div className="flex items-center justify-between px-2 py-1">
        <p className="text-[11px] font-semibold uppercase tracking-wide text-stone-400">{t("skills.available")}</p>
        <span className="font-mono text-[11px] text-stone-400">{items.length}{filterText.trim() ? ` / ${total}` : ""}</span>
      </div>
      {projectMissing ? <p className="px-2 py-5 text-[12px] text-stone-500">{t("skills.projectRequired")}</p> : loading ? <p className="px-2 py-5 text-[12px] text-stone-500">{t("skills.scanning")}</p> : <div className="mt-1 min-h-0 flex-1 space-y-0.5 overflow-y-auto">{items.map((item) => <SkillListRow item={item} selected={item.id === selectedId} onClick={() => onSelect(item)} key={item.id} />)}</div>}
      {locations.length ? <div className="mt-3 max-h-32 shrink-0 overflow-y-auto border-t border-stone-100 px-2 pt-2">{locations.map((location) => <p className="truncate font-mono text-[10px] text-stone-400" key={location.path} title={location.path}>{location.exists ? "●" : "○"} {location.path}</p>)}</div> : null}
    </aside>
  );
}

function SkillListRow({ item, selected, onClick }: { item: SkillItem; selected: boolean; onClick: () => void }) {
  const { t } = useI18n();
  return <button aria-current={selected ? "true" : undefined} aria-label={item.name} className={`w-full border px-2.5 py-2 text-left ${selected ? "border-stone-300 bg-stone-100" : "border-transparent hover:border-stone-200 hover:bg-stone-50"}`} onClick={onClick} type="button"><div className="flex items-center justify-between gap-2"><span className="truncate text-[12px] font-semibold text-stone-900">{item.name}</span>{item.read_only ? <span className="shrink-0 text-[10px] text-stone-400">{t("skills.builtIn")}</span> : null}</div><p className="mt-0.5 truncate font-mono text-[10px] text-stone-500">{item.id}</p>{item.description ? <p className="mt-1 line-clamp-1 text-[11px] text-stone-500">{item.description}</p> : null}</button>;
}
