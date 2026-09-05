import { useEffect, useId, useRef, useState } from "react";
import type { ModelMapping } from "../../lib/api/types";
import { CLAUDE_MENU_ROLES, claudeRoleLabel } from "../../lib/claude-roles";
import {
  codexContextWindowLabel,
  codexEffectiveReasoningLevels,
  normalizeCodexContextWindow,
  usesCodexBaselineReasoning,
} from "../../lib/codexModelCapability";

export type DisplayModelMapping = {
  alias: string;
  target: string;
  label?: string | null;
  oneM: boolean;
  /** Codex-only catalog extras, kept for the tooltip so a config can be checked
   * without opening the editor. */
  contextWindow?: number | null;
  reasoningLevels?: readonly string[] | null;
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

export function expandDisplayModelMappings(
  platform: string,
  mappings: ModelMapping[],
): DisplayModelMapping[] {
  const expanded: DisplayModelMapping[] = [];
  const normalizedPlatform = platform.trim().toLowerCase();
  const isClaude = normalizedPlatform === "claude";
  const isCodex = normalizedPlatform === "codex";

  for (const mapping of mappings) {
    const alias = mapping.from.trim();
    const target = mapping.to.trim();
    if (!alias || !target) {
      continue;
    }
    const label = mapping.label?.trim() || null;
    const normalized: DisplayModelMapping = { alias, target, label, oneM: false };
    if (isCodex) {
      normalized.contextWindow = normalizeCodexContextWindow(mapping.context_window);
      // Only a hand-picked list is worth showing: "the baseline" adds no
      // information the model id does not already carry.
      normalized.reasoningLevels = usesCodexBaselineReasoning(mapping.reasoning_levels)
        ? null
        : codexEffectiveReasoningLevels(alias, mapping.reasoning_levels);
    }
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
  const extras = [
    mapping.contextWindow ? codexContextWindowLabel(mapping.contextWindow) : null,
    mapping.reasoningLevels?.length ? mapping.reasoningLevels.join("/") : null,
  ].filter(Boolean);
  return `${mapping.alias}${oneM} → ${mapping.target}${label ? ` · ${label}` : ""}${
    extras.length ? ` · ${extras.join(" · ")}` : ""
  }`;
}

/** One tag in the summary row, plus the longer line its popover shows. */
type ModelSummaryEntry = {
  key: string;
  /** Tag text. */
  text: string;
  /** `title` tooltip for the tag. */
  detail: string;
  /** Row text inside the 完整模型映射 popover. */
  popoverText: string;
};

/**
 * Claude tags are keyed by upstream model, not by role.
 *
 * Every Claude account fills the same fixed role set, so role-named tags render
 * identically on every card — eight tags that say nothing about which account
 * serves what. The mapping *target* is the part that actually differs between a
 * relay and an official account, so the tag shows `to` and the roles move into
 * the tooltip. Targets are deduped case-insensitively because several roles
 * usually share one upstream model.
 */
function claudeUpstreamEntries(displayMappings: DisplayModelMapping[]): ModelSummaryEntry[] {
  const groups = new Map<string, { target: string; roles: string[]; oneM: boolean }>();
  for (const mapping of displayMappings) {
    const key = mapping.target.toLowerCase();
    const group = groups.get(key) ?? { target: mapping.target, roles: [], oneM: false };
    const role = mapping.label?.trim() || claudeRoleLabel(baseAlias(mapping.alias));
    if (!group.roles.includes(role)) {
      group.roles.push(role);
    }
    group.oneM = group.oneM || mapping.oneM || hasOneMSuffix(mapping.alias);
    groups.set(key, group);
  }

  return Array.from(groups.values()).map((group) => {
    const detail = `${group.roles.join("、")} → ${group.target}${group.oneM ? " · 1M" : ""}`;
    return { key: group.target, text: group.target, detail, popoverText: detail };
  });
}

function modelSummaryEntries(platform: string, mappings: ModelMapping[]): ModelSummaryEntry[] {
  const displayMappings = expandDisplayModelMappings(platform, mappings);
  if (platform.trim().toLowerCase() === "claude") {
    return claudeUpstreamEntries(displayMappings);
  }
  // Everywhere else the alias *is* the client-facing model name, so it is both
  // what the user typed and what distinguishes one account from another.
  return displayMappings.map((mapping, index) => ({
    key: `${mapping.alias}-${mapping.target}-${index}`,
    text: mapping.alias,
    detail: mappingDetail(mapping),
    popoverText: `${mapping.alias} → ${mapping.target}`,
  }));
}

const BASELINE_LABEL = "基线模型";

function baselineTooltipText(platform: string): string {
  const baselineModels = baselineModelsForPlatform(platform).map((alias) => claudeRoleLabel(alias));
  return baselineModels.length > 0
    ? "未配置模型映射，仅匹配基线模型：" + baselineModels.join("、")
    : "未配置模型映射，当前平台暂无预设基线模型";
}

/**
 * The same summary the tag row shows, flattened to one dot-separated line.
 *
 * For places that have a line of running text rather than room for tags — the
 * account row's stats line doubles as this when the tag row is switched off. It
 * deliberately shares `modelSummaryEntries` so the two can never disagree about
 * which models an account serves.
 */
export function modelSummaryLine(
  platform: string,
  mappings: ModelMapping[],
): { text: string; title: string } {
  const entries = modelSummaryEntries(platform, mappings);
  if (entries.length === 0) {
    return { text: BASELINE_LABEL, title: baselineTooltipText(platform) };
  }
  return {
    text: entries.map((entry) => entry.text).join(" · "),
    title: entries.map((entry) => entry.popoverText).join("\n"),
  };
}

export function ModelMappingSummary({
  platform,
  mappings,
}: {
  platform: string;
  mappings: ModelMapping[];
}): JSX.Element {
  const entries = modelSummaryEntries(platform, mappings);
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

  if (entries.length === 0) {
    return (
      <span
        aria-describedby={baselineTooltipId}
        className="group relative inline-flex rounded-full bg-[#f7f7f7] px-2 py-0.5 text-[10px] text-[#666] outline-none focus:ring-2 focus:ring-stone-300"
        tabIndex={0}
      >
        <span>{BASELINE_LABEL}</span>
        <span
          className="pointer-events-none absolute left-0 top-full z-50 mt-1 hidden w-64 max-w-[calc(100vw-2rem)] whitespace-normal break-words rounded-lg border border-stone-200 bg-stone-900 px-3 py-2 text-left text-[11px] font-medium leading-5 text-white shadow-xl group-hover:block group-focus-within:block"
          id={baselineTooltipId}
          role="tooltip"
        >
          {baselineTooltipText(platform)}
        </span>
      </span>
    );
  }

  const visibleEntries = entries.slice(0, 3);
  const remainingCount = entries.length - visibleEntries.length;

  return (
    <div ref={containerRef} className="relative flex min-w-0 flex-wrap items-center gap-1">
      {visibleEntries.map((entry) => (
        <span
          className="inline-flex max-w-48 truncate rounded-full bg-[#f7f7f7] px-2 py-0.5 font-mono text-[9px] text-[#666]"
          key={entry.key}
          title={entry.detail}
        >
          {entry.text}
        </span>
      ))}
      {remainingCount > 0 ? (
        <button
          aria-expanded={open}
          aria-haspopup="dialog"
          // Same palette as the tags it stands in for, one shade darker on hover so
          // it still reads as the one clickable thing in the row.
          className="rounded-full bg-[#f7f7f7] px-2 py-0.5 font-mono text-[9px] text-[#666] motion-control hover:bg-[#ededed]"
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
            {entries.map((entry) => (
              <p
                className="truncate rounded-lg bg-stone-50 px-2 py-1 font-mono text-stone-700"
                key={entry.key}
                title={entry.detail}
              >
                {entry.popoverText}
              </p>
            ))}
          </div>
        </div>
      ) : null}
    </div>
  );
}
