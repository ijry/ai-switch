import { ChevronDown, ChevronRight } from "lucide-react";
import { useState } from "react";
import type { BatchGroup } from "../../lib/api/types";

type BatchListProps = {
  groups: BatchGroup[];
  search: string;
};

const statusClassName: Record<BatchGroup["health"], string> = {
  ok: "bg-moss/10 text-moss",
  warning: "bg-sun/20 text-ink",
  error: "bg-ember/10 text-ember",
};

export function BatchList({ groups, search }: BatchListProps) {
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const normalizedSearch = search.trim().toLowerCase();

  if (groups.length === 0) {
    return (
      <div className="rounded-3xl border border-dashed border-ink/20 bg-white/55 p-8 text-center text-steel">
        No batches or records yet.
      </div>
    );
  }

  return (
    <div className="space-y-3">
      {groups.map((group) => {
        const searchMatchesChild =
          normalizedSearch.length > 0 &&
          group.children.some((child) =>
            `${child.title} ${child.subtitle ?? ""} ${child.item_type}`
              .toLowerCase()
              .includes(normalizedSearch),
          );
        const isExpanded = expanded[group.batch.id] || searchMatchesChild;

        return (
          <section
            key={group.batch.id}
            className="rounded-3xl border border-ink/10 bg-white/80 p-4 shadow-sm shadow-ink/5"
          >
            <button
              type="button"
              aria-label={`expand ${group.batch.name}`}
              className="flex w-full cursor-pointer items-center justify-between gap-4 rounded-2xl text-left outline-none transition-colors duration-200 focus-visible:ring-2 focus-visible:ring-moss/50"
              onClick={() =>
                setExpanded((current) => ({
                  ...current,
                  [group.batch.id]: !isExpanded,
                }))
              }
            >
              <span className="flex items-center gap-3">
                <span className="flex h-9 w-9 items-center justify-center rounded-2xl bg-paper text-ink">
                  {isExpanded ? <ChevronDown size={18} /> : <ChevronRight size={18} />}
                </span>
                <span>
                  <span className="block font-display text-lg font-semibold text-ink">
                    {group.batch.name}
                  </span>
                  <span className="text-sm text-steel">
                    {group.batch.source} · {group.children.length} item
                    {group.children.length === 1 ? "" : "s"}
                  </span>
                </span>
              </span>
              <span
                className={`rounded-full px-3 py-1 text-xs font-semibold uppercase tracking-wide ${
                  statusClassName[group.health]
                }`}
              >
                {group.health}
              </span>
            </button>

            {isExpanded && (
              <div className="mt-4 divide-y divide-ink/10 overflow-hidden rounded-2xl border border-ink/10">
                {group.children.map((child) => (
                  <div
                    key={`${child.item_type}:${child.id}`}
                    className="flex items-center justify-between gap-4 bg-paper/60 px-4 py-3"
                  >
                    <div>
                      <p className="font-medium text-ink">{child.title}</p>
                      <p className="text-sm text-steel">{child.subtitle ?? child.item_type}</p>
                    </div>
                    <span className="rounded-full bg-white px-3 py-1 text-xs text-steel">
                      {child.item_type}
                    </span>
                  </div>
                ))}
              </div>
            )}
          </section>
        );
      })}
    </div>
  );
}
