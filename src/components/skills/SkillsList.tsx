import { AnimatePresence } from "motion/react";
import type { SkillItem, SkillLocation } from "../../lib/api/types";
import { MotionListItem } from "../motion/MotionPrimitives";
import { useI18n } from "../../lib/i18n";
import { skillDisplayCopy } from "./catalog";

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
    <aside className="flex min-h-0 min-w-0 flex-col overflow-hidden rounded-2xl border border-stone-200 bg-white p-2 shadow-sm">
      <div className="flex items-center justify-between gap-2 px-2 py-1">
        <p className="truncate text-[11px] font-semibold uppercase tracking-wide text-stone-400">{t("skills.available")}</p>
        <span className="shrink-0 font-mono text-[11px] text-stone-400">{items.length}{filterText.trim() ? ` / ${total}` : ""}</span>
      </div>
      {projectMissing ? <p className="px-2 py-5 text-[12px] text-stone-500">{t("skills.projectRequired")}</p> : loading ? <p className="px-2 py-5 text-[12px] text-stone-500">{t("skills.scanning")}</p> : items.length ? <div className="mt-1 min-h-0 flex-1 space-y-0.5 overflow-y-auto"><AnimatePresence initial={false}>{items.map((item) => <MotionListItem itemKey={item.id} key={item.id}><SkillListRow item={item} selected={item.id === selectedId} onClick={() => onSelect(item)} /></MotionListItem>)}</AnimatePresence></div> : <p className="px-2 py-5 text-[12px] text-stone-500">{filterText.trim() ? t("skills.noMatches") : t("skills.empty")}</p>}
      {locations.length ? <div className="mt-3 max-h-32 shrink-0 overflow-y-auto border-t border-stone-100 px-2 pt-2">{locations.map((location) => <p className="truncate font-mono text-[10px] text-stone-400" key={location.path} title={location.path}>{location.exists ? "●" : "○"} {location.path}</p>)}</div> : null}
    </aside>
  );
}

function SkillListRow({ item, selected, onClick }: { item: SkillItem; selected: boolean; onClick: () => void }) {
  const { language, t } = useI18n();
  const copy = skillDisplayCopy(item, language);
  return <button aria-current={selected ? "true" : undefined} aria-label={copy.name} className={`w-full rounded-xl px-2.5 py-2 text-left motion-control ${selected ? "bg-stone-100 ring-1 ring-stone-300" : "hover:bg-stone-50"}`} onClick={onClick} type="button"><div className="flex items-center justify-between gap-2"><span className="truncate text-[12px] font-semibold text-stone-900">{copy.name}</span>{item.read_only ? <span className="shrink-0 whitespace-nowrap text-[10px] text-stone-400">{t("skills.builtIn")}</span> : null}</div><p className="mt-0.5 truncate font-mono text-[10px] text-stone-500">{item.id}</p>{copy.description ? <p className="mt-1 line-clamp-2 text-[11px] text-stone-500">{copy.description}</p> : null}</button>;
}
