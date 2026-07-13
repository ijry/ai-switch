import { ChevronDown, ChevronRight } from "lucide-react";
import { useState } from "react";
import type { BatchGroup } from "../../lib/api/types";

type BatchListProps = {
  groups: BatchGroup[];
  search: string;
};

const statusClassName: Record<BatchGroup["health"], string> = {
  ok: "bg-emerald-50 text-emerald-700",
  warning: "bg-amber-50 text-amber-700",
  error: "bg-red-50 text-red-700",
};

export function BatchList({ groups, search }: BatchListProps) {
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const normalizedSearch = search.trim().toLowerCase();

  if (groups.length === 0) {
    return (
      <div className="rounded-xl border border-dashed border-stone-300 bg-white/70 p-6 text-center text-sm text-stone-500">
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
            className="rounded-xl border border-stone-200 bg-white/82 p-3 shadow-sm"
          >
            <button
              type="button"
              aria-label={`expand ${group.batch.name}`}
              className="flex w-full items-center justify-between gap-3 rounded-xl text-left outline-none transition-colors duration-150 focus-visible:ring-2 focus-visible:ring-blue-400"
              onClick={() =>
                setExpanded((current) => ({
                  ...current,
                  [group.batch.id]: !isExpanded,
                }))
              }
            >
              <span className="flex min-w-0 items-center gap-2.5">
                <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-stone-100 text-stone-700">
                  {isExpanded ? <ChevronDown size={16} /> : <ChevronRight size={16} />}
                </span>
                <span className="min-w-0">
                  <span className="block truncate text-[13px] font-semibold text-stone-950">
                    {group.batch.name}
                  </span>
                  <span className="text-[12px] text-stone-500">
                    {group.batch.source} · {group.children.length} item
                    {group.children.length === 1 ? "" : "s"}
                  </span>
                </span>
              </span>
              <span
                className={`rounded-full px-2 py-0.5 text-[11px] font-semibold uppercase tracking-wide ${
                  statusClassName[group.health]
                }`}
              >
                {group.health}
              </span>
            </button>

            {isExpanded && (
              <div className="mt-3 divide-y divide-stone-100 overflow-hidden rounded-xl border border-stone-200">
                {group.children.map((child) => (
                  <div
                    key={`${child.item_type}:${child.id}`}
                    className="flex items-center justify-between gap-3 bg-stone-50/70 px-3 py-2"
                  >
                    <div className="min-w-0">
                      <p className="truncate text-[13px] font-medium text-stone-950">{child.title}</p>
                      <p className="truncate text-[12px] text-stone-500">{child.subtitle ?? child.item_type}</p>
                    </div>
                    <span className="rounded-full bg-white px-2 py-0.5 text-[11px] text-stone-500">
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
