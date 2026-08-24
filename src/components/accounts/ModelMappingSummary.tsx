import { useEffect, useId, useRef, useState } from "react";
import type { ModelMapping } from "../../lib/api/types";
import { CLAUDE_MENU_ROLES, claudeRoleLabel } from "../../lib/claude-roles";

export type DisplayModelMapping = {
  alias: string;
  target: string;
  label?: string | null;
  oneM: boolean;
};

const baselineModelsByPlatform: Record<string, readonly string[]> = {
  codex: ["gpt-5.6-sol", "gpt-5.6-terra", "gpt-5.6-luna", "gpt-5.5"],
  claude: CLAUDE_MENU_ROLES.map((role) => role.alias),
  gemini: ["gemini-2.5-flash"],
  grok: ["grok-4.5"],
};

export function baselineModelsForPlatform(platform: string) {
  return baselineModelsByPlatform[platform.trim().toLowerCase()] ?? [];
}

function hasOneMSuffix(value: string) {
  return value.toLowerCase().endsWith("[1m]");
}

/** Strips a `[1m]` suffix so an alias can be matched against the role table. */
function baseAlias(alias: string) {
  return hasOneMSuffix(alias) ? alias.slice(0, -"[1m]".length) : alias;
}

/**
 * Human-facing title for one mapping row.
 *
 * Shows the role name rather than the internal alias. The alias is an
 * implementation detail, and pairing it with the upstream model produced lines
 * like `claude-opus-alias → claude-opus-5` that read as a misconfiguration. A
 * hand-written alias has no role and is shown verbatim — that is what the user
 * typed.
 */
export function displayMappingTitle(mapping: DisplayModelMapping) {
  const role = claudeRoleLabel(baseAlias(mapping.alias));
  return hasOneMSuffix(mapping.alias) || mapping.oneM ? `${role} · 1M` : role;
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

/// Tooltip detail: keeps the internal alias reachable for troubleshooting.
function mappingDetail(mapping: DisplayModelMapping) {
  const label = mapping.label?.trim();
  const oneM = mapping.oneM && !hasOneMSuffix(mapping.alias) ? "[1m]" : "";
  return `${mapping.alias}${oneM} → ${mapping.target}${label ? ` · ${label}` : ""}`;
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
  const baselineTooltipId = useId();

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
    const baselineModels = baselineModelsForPlatform(platform).map((alias) =>
      claudeRoleLabel(alias),
    );
    const baselineTooltipText = baselineModels.length > 0
      ? "未配置模型映射，仅匹配基线模型：" + baselineModels.join("、")
      : "未配置模型映射，当前平台暂无预设基线模型";
    return (
      <span
        aria-describedby={baselineTooltipId}
        className="group relative inline-flex rounded-full bg-stone-100 px-2 py-0.5 text-[11px] font-semibold text-stone-600 outline-none focus:ring-2 focus:ring-stone-300"
        tabIndex={0}
      >
        <span>基线模型</span>
        <span
          className="pointer-events-none absolute left-0 top-full z-50 mt-1 hidden w-64 max-w-[calc(100vw-2rem)] whitespace-normal break-words rounded-lg border border-stone-200 bg-stone-900 px-3 py-2 text-left text-[11px] font-medium leading-5 text-white shadow-xl group-hover:block group-focus-within:block"
          id={baselineTooltipId}
          role="tooltip"
        >
          {baselineTooltipText}
        </span>
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
          {displayMappingTitle(mapping)}
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
                {displayMappingTitle(mapping)} → {mapping.target}
              </p>
            ))}
          </div>
        </div>
      ) : null}
    </div>
  );
}
