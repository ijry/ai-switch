import { useEffect, useRef, useState } from "react";
import type { ModelMapping } from "../../lib/api/types";

export type DisplayModelMapping = {
  alias: string;
  target: string;
  label?: string | null;
  oneM: boolean;
};

function hasOneMSuffix(value: string) {
  return value.toLowerCase().endsWith("[1m]");
}

export function expandDisplayModelMappings(
  platform: string,
  mappings: ModelMapping[],
): DisplayModelMapping[] {
  const expanded: DisplayModelMapping[] = [];
  const isClaude = platform.trim().toLowerCase() === "claude";

  for (const mapping of mappings) {
    const alias = mapping.from.trim();
    const target = mapping.to.trim();
    if (!alias || !target) {
      continue;
    }
    const label = mapping.label?.trim() || null;
    const normalized = { alias, target, label, oneM: false };
    expanded.push(normalized);

    if (isClaude && mapping.supports_1m === true && !hasOneMSuffix(alias)) {
      expanded.push({
        alias: `${alias}[1m]`,
        target,
        label,
        oneM: true,
      });
    }
  }

  return expanded;
}

function mappingDetail(mapping: DisplayModelMapping) {
  const label = mapping.label?.trim();
  const oneM = mapping.oneM && !hasOneMSuffix(mapping.alias) ? " · [1m]" : "";
  return `${mapping.alias} → ${mapping.target}${label ? ` · ${label}` : ""}${oneM}`;
}

export function ModelMappingSummary({
  platform,
  mappings,
}: {
  platform: string;
  mappings: ModelMapping[];
}): JSX.Element {
  const displayMappings = expandDisplayModelMappings(platform, mappings);
  const [open, setOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) {
      return;
    }

    const handlePointerDown = (event: PointerEvent) => {
      if (!containerRef.current?.contains(event.target as Node)) {
        setOpen(false);
      }
    };
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setOpen(false);
      }
    };

    document.addEventListener("pointerdown", handlePointerDown);
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("pointerdown", handlePointerDown);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [open]);

  if (displayMappings.length === 0) {
    return (
      <span className="rounded-full bg-stone-100 px-2 py-0.5 text-[11px] font-semibold text-stone-600">
        模型通配
      </span>
    );
  }

  const visibleMappings = displayMappings.slice(0, 3);
  const remainingCount = displayMappings.length - visibleMappings.length;

  return (
    <div ref={containerRef} className="relative flex min-w-0 flex-wrap items-center gap-1">
      {visibleMappings.map((mapping) => (
        <span
          className="inline-flex max-w-48 truncate rounded-full bg-sky-50 px-2 py-0.5 font-mono text-[10px] font-semibold text-sky-800"
          key={`${mapping.alias}-${mapping.target}`}
          title={mappingDetail(mapping)}
        >
          {mapping.alias}
        </span>
      ))}
      {remainingCount > 0 ? (
        <button
          aria-expanded={open}
          aria-haspopup="dialog"
          className="rounded-full bg-sky-100 px-2 py-0.5 font-mono text-[10px] font-semibold text-sky-900 transition-colors hover:bg-sky-200"
          onClick={() => setOpen((current) => !current)}
          title="查看完整模型映射"
          type="button"
        >
          +{remainingCount}
        </button>
      ) : null}
      {open ? (
        <div
          aria-label="完整模型映射"
          className="absolute left-0 top-full z-40 mt-1 grid max-h-64 min-w-64 max-w-[min(30rem,calc(100vw-2rem))] gap-2 overflow-y-auto rounded-xl border border-stone-200 bg-white p-3 text-[11px] shadow-xl"
          role="dialog"
        >
          <p className="font-semibold text-stone-700">完整模型映射</p>
          <div className="grid gap-1.5">
            {displayMappings.map((mapping, index) => (
              <p
                className="truncate rounded-lg bg-stone-50 px-2 py-1 font-mono text-stone-700"
                key={`${mapping.alias}-${mapping.target}-${index}`}
                title={mappingDetail(mapping)}
              >
                {mappingDetail(mapping)}
              </p>
            ))}
          </div>
        </div>
      ) : null}
    </div>
  );
}
