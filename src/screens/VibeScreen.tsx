import { useQuery } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import {
  ChevronDown,
  ChevronRight,
  FolderOpen,
  MoonStar,
  Palette,
  PanelLeftClose,
  Play,
  Plus,
  SendHorizontal,
  SunMedium,
  TerminalSquare,
  Upload,
  X,
} from "lucide-react";
import { useCallback, useEffect, useId, useMemo, useRef, useState } from "react";
import type { ChangeEvent, ReactNode } from "react";
import { createTerminalSession, killTerminalSession, listSessions } from "../lib/api/client";
import { useI18n } from "../lib/i18n";
import {
  BUILT_IN_VIBE_SKINS,
  clearStoredVibeSkin,
  getVibeSkinBlocks,
  importVibeSkinPackage,
  VIBE_SKIN_REGION_KEYS,
  readStoredVibeSkin,
  skinToCssVariables,
  writeStoredVibeSkin,
} from "../lib/vibeSkin";
import type {
  VibeSkinDecorationCard,
  VibeSkinDecorationItem,
  VibeSkinDecorationTemplate,
  VibeSkinDecorationTone,
  VibeSkinDecorationVariant,
  VibeSkinDefinition,
  VibeSkinTaskbarMenuItem,
} from "../lib/vibeSkin";
import { AiSwitchLogo } from "../components/brand/AiSwitchLogo";
import type {
  CreateTerminalSessionInput,
  SessionMeta,
  TerminalSession,
  TerminalStatus,
} from "../lib/api/types";
import { XtermPane } from "../components/terminal/XtermPane";
import { StarshipHologram } from "../components/vibe/StarshipHologram";

const agentOptions = [
  { platform: "codex", label: "Codex" },
  { platform: "claude", label: "Claude" },
  { platform: "gemini", label: "Gemini" },
  { platform: "opencode", label: "OpenCode" },
  { platform: "openclaw", label: "OpenClaw" },
  { platform: "hermes", label: "Hermes" },
] as const;

const launchModelOptions = [
  { value: "auto", label: "Auto" },
  { value: "gpt-5", label: "GPT-5" },
  { value: "claude-sonnet-4", label: "Claude Sonnet 4" },
  { value: "gemini-2.5-pro", label: "Gemini 2.5 Pro" },
] as const;

const launchReasoningOptions = [
  { value: "auto", label: "Auto" },
  { value: "low", label: "Low" },
  { value: "medium", label: "Medium" },
  { value: "high", label: "High" },
] as const;

const chooseFolderOptionValue = "__choose_folder__";

type AgentPlatform = (typeof agentOptions)[number]["platform"];

type VibeTheme = "dark" | "light" | "skin";

type VibeScreenProps = {
  onExitVibe?: () => void;
};

type SessionDirectoryDisplay = {
  key: string;
  label: string;
  title: string;
};

type SessionGroup = SessionDirectoryDisplay & {
  items: SessionMeta[];
};

const isoDateSegmentPattern = /^\d{4}-\d{2}-\d{2}$/;
const yearSegmentPattern = /^\d{4}$/;
const monthOrDaySegmentPattern = /^\d{2}$/;
const uuidSegmentPattern =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const compactUuidSegmentPattern = /^[0-9a-f]{24,}$/i;

function pathSegments(directory: string) {
  return directory.trim().split(/[\\/]+/).filter(Boolean);
}

function joinDisplayPath(parts: string[]) {
  return parts.join("/");
}

function isDateSegment(segment: string) {
  return isoDateSegmentPattern.test(segment);
}

function isDateTriplet(parts: string[], index: number) {
  const year = parts[index];
  const month = parts[index + 1];
  const day = parts[index + 2];
  if (!year || !month || !day) {
    return false;
  }

  return (
    yearSegmentPattern.test(year) &&
    monthOrDaySegmentPattern.test(month) &&
    monthOrDaySegmentPattern.test(day)
  );
}

function dateLabelFromTriplet(parts: string[], index: number) {
  return `${parts[index]}-${parts[index + 1]}-${parts[index + 2]}`;
}

function isOpaqueSessionSegment(segment: string) {
  return uuidSegmentPattern.test(segment) || compactUuidSegmentPattern.test(segment);
}

function stripTrailingOpaqueSegments(parts: string[]) {
  let end = parts.length;
  while (end > 0 && isOpaqueSessionSegment(parts[end - 1] ?? "")) {
    end -= 1;
  }
  return parts.slice(0, end);
}

function datedDirectoryDisplay(directory: string): SessionDirectoryDisplay | null {
  const parts = pathSegments(directory);

  for (let index = parts.length - 2; index >= 0; index -= 1) {
    if (!isDateSegment(parts[index] ?? "")) {
      continue;
    }

    const dateLabel = parts[index] ?? "";
    const parentParts = parts.slice(0, index);
    const childParts = parts.slice(index + 1);
    return directoryDisplayFromDateParts(parentParts, dateLabel, childParts);
  }

  for (let index = parts.length - 4; index >= 0; index -= 1) {
    if (!isDateTriplet(parts, index)) {
      continue;
    }

    const dateLabel = dateLabelFromTriplet(parts, index);
    const parentParts = parts.slice(0, index);
    const childParts = parts.slice(index + 3);
    return directoryDisplayFromDateParts(parentParts, dateLabel, childParts);
  }

  return null;
}

function directoryDisplayFromDateParts(
  parentParts: string[],
  dateLabel: string,
  childParts: string[],
): SessionDirectoryDisplay | null {
  if (childParts.length === 0) {
    return null;
  }

  const datePath = joinDisplayPath([...parentParts, dateLabel]);
  if (isOpaqueSessionSegment(childParts[0] ?? "")) {
    return {
      key: `date:${datePath.toLowerCase()}`,
      label: dateLabel,
      title: datePath,
    };
  }

  const meaningfulChildParts = stripTrailingOpaqueSegments(childParts);
  const label = joinDisplayPath(meaningfulChildParts);
  if (!label) {
    return {
      key: `date:${datePath.toLowerCase()}`,
      label: dateLabel,
      title: datePath,
    };
  }

  return {
    key: `dated-name:${joinDisplayPath(parentParts).toLowerCase()}:${label.toLowerCase()}`,
    label,
    title: label,
  };
}

function directoryDisplay(session: SessionMeta, unknownLabel: string): SessionDirectoryDisplay {
  const directory = directoryLabel(session, unknownLabel);
  const datedDisplay = datedDirectoryDisplay(directory);
  if (datedDisplay) {
    return datedDisplay;
  }

  return {
    key: `directory:${directory}`,
    label: compactDirectoryLabel(directory),
    title: directory,
  };
}

function titleForSession(session: SessionMeta, unknownLabel = "Unknown directory") {
  return session.title?.trim() || directoryDisplay(session, unknownLabel).label || session.sessionId;
}

function directoryLabel(session: SessionMeta, unknownLabel: string) {
  return session.projectDir?.trim() || unknownLabel;
}

function compactDirectoryLabel(directory: string) {
  const trimmed = directory.trim();
  const parts = trimmed.split(/[\\/]+/).filter(Boolean);
  if (parts.length < 2) {
    return directory;
  }
  return parts.slice(-2).join("/");
}

function groupSessions(sessions: SessionMeta[], unknownLabel: string) {
  const groups = new Map<string, SessionGroup>();
  for (const session of sessions) {
    const display = directoryDisplay(session, unknownLabel);
    const current = groups.get(display.key);
    groups.set(display.key, {
      ...display,
      items: [...(current?.items ?? []), session],
    });
  }
  return Array.from(groups.values());
}

function formatError(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function sessionKey(session: SessionMeta) {
  return `${session.providerId}:${session.sessionId}:${session.sourcePath}`;
}

function shortTabTitle(title: string) {
  const cleaned = title.trim();
  if (!cleaned) {
    return "Terminal";
  }

  const parts = cleaned.split(/[\\/]/).filter(Boolean);
  if (parts.length === 0) {
    return cleaned;
  }

  const leaf = parts[parts.length - 1] ?? cleaned;
  if (cleaned.includes(" - ") && parts.length > 1) {
    const agent = cleaned.split(" - ")[0]?.trim();
    if (agent) {
      return `${agent} · ${leaf}`;
    }
  }
  return leaf;
}

type AgentIconProps = {
  platform: AgentPlatform;
  className?: string;
};

type AgentSvgProps = {
  size?: string | number;
};

const baseAgentSvgStyle = { flex: "none", lineHeight: 1 } as const;

function useSvgGradientId(prefix: string) {
  return `${prefix}-${useId().replace(/:/g, "")}`;
}

function CodexAgentSvg({ size = "100%" }: AgentSvgProps) {
  const gradientId = useSvgGradientId("codex-agent");
  return (
    <svg
      height={size}
      style={baseAgentSvgStyle}
      viewBox="2 2 20 20"
      width={size}
      xmlns="http://www.w3.org/2000/svg"
    >
      <path
        d="M9.064 3.344a4.578 4.578 0 012.285-.312c1 .115 1.891.54 2.673 1.275.01.01.024.017.037.021a.09.09 0 00.043 0 4.55 4.55 0 013.046.275l.047.022.116.057a4.581 4.581 0 012.188 2.399c.209.51.313 1.041.315 1.595a4.24 4.24 0 01-.134 1.223.123.123 0 00.03.115c.594.607.988 1.33 1.183 2.17.289 1.425-.007 2.71-.887 3.854l-.136.166a4.548 4.548 0 01-2.201 1.388.123.123 0 00-.081.076c-.191.551-.383 1.023-.74 1.494-.9 1.187-2.222 1.846-3.711 1.838-1.187-.006-2.239-.44-3.157-1.302a.107.107 0 00-.105-.024c-.388.125-.78.143-1.204.138a4.441 4.441 0 01-1.945-.466 4.544 4.544 0 01-1.61-1.335c-.152-.202-.303-.392-.414-.617a5.81 5.81 0 01-.37-.961 4.582 4.582 0 01-.014-2.298.124.124 0 00.006-.056.085.085 0 00-.027-.048 4.467 4.467 0 01-1.034-1.651 3.896 3.896 0 01-.251-1.192 5.189 5.189 0 01.141-1.6c.337-1.112.982-1.985 1.933-2.618.212-.141.413-.251.601-.33.215-.089.43-.164.646-.227a.098.098 0 00.065-.066 4.51 4.51 0 01.829-1.615 4.535 4.535 0 011.837-1.388zm3.482 10.565a.637.637 0 000 1.272h3.636a.637.637 0 100-1.272h-3.636zM8.462 9.23a.637.637 0 00-1.106.631l1.272 2.224-1.266 2.136a.636.636 0 101.095.649l1.454-2.455a.636.636 0 00.005-.64L8.462 9.23z"
        fill={`url(#${gradientId})`}
      />
      <defs>
        <linearGradient
          gradientUnits="userSpaceOnUse"
          id={gradientId}
          x1="12"
          x2="12"
          y1="3"
          y2="21"
        >
          <stop stopColor="#B1A7FF" />
          <stop offset=".5" stopColor="#7A9DFF" />
          <stop offset="1" stopColor="#3941FF" />
        </linearGradient>
      </defs>
    </svg>
  );
}

function ClaudeAgentSvg({ size = "100%" }: AgentSvgProps) {
  return (
    <svg
      height={size}
      style={baseAgentSvgStyle}
      viewBox="0 0 24 24"
      width={size}
      xmlns="http://www.w3.org/2000/svg"
    >
      <path
        clipRule="evenodd"
        d="M20.998 10.949H24v3.102h-3v3.028h-1.487V20H18v-2.921h-1.487V20H15v-2.921H9V20H7.488v-2.921H6V20H4.487v-2.921H3V14.05H0V10.95h3V5h17.998v5.949zM6 10.949h1.488V8.102H6v2.847zm10.51 0H18V8.102h-1.49v2.847z"
        fill="#D97757"
        fillRule="evenodd"
      />
    </svg>
  );
}

function GeminiAgentSvg({ size = "100%" }: AgentSvgProps) {
  const gradientId = useSvgGradientId("gemini-agent");
  return (
    <svg
      height={size}
      style={baseAgentSvgStyle}
      viewBox="0 0 24 24"
      width={size}
      xmlns="http://www.w3.org/2000/svg"
    >
      <path
        d="M0 4.391A4.391 4.391 0 014.391 0h15.217A4.391 4.391 0 0124 4.391v15.217A4.391 4.391 0 0119.608 24H4.391A4.391 4.391 0 010 19.608V4.391z"
        fill={`url(#${gradientId})`}
      />
      <path
        clipRule="evenodd"
        d="M19.74 1.444a2.816 2.816 0 012.816 2.816v15.48a2.816 2.816 0 01-2.816 2.816H4.26a2.816 2.816 0 01-2.816-2.816V4.26A2.816 2.816 0 014.26 1.444h15.48zM7.236 8.564l7.752 3.728-7.752 3.727v2.802l9.557-4.596v-3.866L7.236 5.763v2.801z"
        fill="#1E1E2E"
        fillRule="evenodd"
      />
      <defs>
        <linearGradient
          gradientUnits="userSpaceOnUse"
          id={gradientId}
          x1="24"
          x2="0"
          y1="6.587"
          y2="16.494"
        >
          <stop stopColor="#EE4D5D" />
          <stop offset=".328" stopColor="#B381DD" />
          <stop offset=".476" stopColor="#207CFE" />
        </linearGradient>
      </defs>
    </svg>
  );
}

function OpenCodeAgentSvg({ size = "100%" }: AgentSvgProps) {
  return (
    <svg
      fill="currentColor"
      fillRule="evenodd"
      height={size}
      style={baseAgentSvgStyle}
      viewBox="0 0 24 24"
      width={size}
      xmlns="http://www.w3.org/2000/svg"
    >
      <path d="M16 6H8v12h8V6zm4 16H4V2h16v20z" />
    </svg>
  );
}

function OpenClawAgentSvg({ size = "100%" }: AgentSvgProps) {
  const shellId = useSvgGradientId("openclaw-agent-shell");
  const leftId = useSvgGradientId("openclaw-agent-left");
  const rightId = useSvgGradientId("openclaw-agent-right");
  return (
    <svg
      height={size}
      style={baseAgentSvgStyle}
      viewBox="0 0 24 24"
      width={size}
      xmlns="http://www.w3.org/2000/svg"
    >
      <path
        d="M12 2.568c-6.33 0-9.495 5.275-9.495 9.495 0 4.22 3.165 8.44 6.33 9.494v2.11h2.11v-2.11s1.055.422 2.11 0v2.11h2.11v-2.11c3.165-1.055 6.33-5.274 6.33-9.494S18.33 2.568 12 2.568z"
        fill={`url(#${shellId})`}
      />
      <path
        d="M3.56 9.953C.396 8.898-.66 11.008.396 13.118c1.055 2.11 3.164 1.055 4.22-1.055.632-1.477 0-2.11-1.056-2.11z"
        fill={`url(#${leftId})`}
      />
      <path
        d="M20.44 9.953c3.164-1.055 4.22 1.055 3.164 3.165-1.055 2.11-3.164 1.055-4.22-1.055-.632-1.477 0-2.11 1.056-2.11z"
        fill={`url(#${rightId})`}
      />
      <path
        d="M5.507 1.875c.476-.285 1.036-.233 1.615.037.577.27 1.223.774 1.937 1.488a.316.316 0 01-.447.447c-.693-.693-1.279-1.138-1.757-1.361-.475-.222-.795-.205-1.022-.069a.317.317 0 01-.326-.542zM16.877 1.913c.58-.27 1.14-.323 1.616-.038a.317.317 0 01-.326.542c-.227-.136-.547-.153-1.022.069-.478.223-1.064.668-1.756 1.361a.316.316 0 11-.448-.447c.714-.714 1.36-1.218 1.936-1.487z"
        fill="#FF4D4D"
      />
      <path
        d="M8.835 9.109a1.266 1.266 0 100-2.532 1.266 1.266 0 000 2.532zM15.165 9.109a1.266 1.266 0 100-2.532 1.266 1.266 0 000 2.532z"
        fill="#050810"
      />
      <path
        d="M9.046 8.16a.527.527 0 100-1.056.527.527 0 000 1.055zM15.376 8.16a.527.527 0 100-1.055.527.527 0 000 1.054z"
        fill="#00E5CC"
      />
      <defs>
        <linearGradient gradientUnits="userSpaceOnUse" id={shellId} x1="-.659" x2="27.023" y1=".458" y2="22.855">
          <stop stopColor="#FF4D4D" />
          <stop offset="1" stopColor="#991B1B" />
        </linearGradient>
        <linearGradient gradientUnits="userSpaceOnUse" id={leftId} x1="0" x2="4.311" y1="9.672" y2="14.949">
          <stop stopColor="#FF4D4D" />
          <stop offset="1" stopColor="#991B1B" />
        </linearGradient>
        <linearGradient gradientUnits="userSpaceOnUse" id={rightId} x1="19.385" x2="24.399" y1="9.953" y2="14.462">
          <stop stopColor="#FF4D4D" />
          <stop offset="1" stopColor="#991B1B" />
        </linearGradient>
      </defs>
    </svg>
  );
}

function HermesAgentSvg({ size = "100%" }: AgentSvgProps) {
  return (
    <svg
      fill="none"
      height={size}
      style={baseAgentSvgStyle}
      viewBox="0 0 24 24"
      width={size}
      xmlns="http://www.w3.org/2000/svg"
    >
      <rect height="20" rx="6" stroke="currentColor" strokeWidth="2" width="20" x="2" y="2" />
      <path
        d="M7 16V8M17 8v8M8 12h8M5 7.2l2.4-2.1M19 7.2l-2.4-2.1"
        stroke="currentColor"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="2"
      />
    </svg>
  );
}

function AgentIcon({ platform, className }: AgentIconProps) {
  const iconClassName = ["inline-flex shrink-0", className].filter(Boolean).join(" ");
  const icon =
    platform === "codex" ? (
      <CodexAgentSvg />
    ) : platform === "claude" ? (
      <ClaudeAgentSvg />
    ) : platform === "gemini" ? (
      <GeminiAgentSvg />
    ) : platform === "opencode" ? (
      <OpenCodeAgentSvg />
    ) : platform === "openclaw" ? (
      <OpenClawAgentSvg />
    ) : (
      <HermesAgentSvg />
    );

  return (
    <span aria-hidden="true" className={iconClassName}>
      {icon}
    </span>
  );
}

function statusDotClass(status: TerminalStatus, isActive: boolean, isDark: boolean) {
  if (status === "running") {
    return isActive ? "bg-emerald-400" : isDark ? "bg-[#5eead4]" : "bg-emerald-400";
  }
  if (status === "error") {
    return isActive ? "bg-red-500" : isDark ? "bg-[#fb7185]" : "bg-red-400";
  }
  return isActive ? "bg-slate-400" : isDark ? "bg-[#64748b]" : "bg-stone-500";
}

function statusLabel(status: TerminalStatus, t: (key: "vibe.status.running" | "vibe.status.exited" | "vibe.status.error") => string) {
  return t(`vibe.status.${status}` as "vibe.status.running" | "vibe.status.exited" | "vibe.status.error");
}

function formatTaskbarClock(date: Date) {
  const hours = String(date.getHours()).padStart(2, "0");
  const minutes = String(date.getMinutes()).padStart(2, "0");
  return `${hours}:${minutes}`;
}

function skinVariantClass(variant: VibeSkinDecorationVariant | undefined) {
  return variant ? `vibe-skin--${variant}` : "";
}

function renderRescueDog(label: string, tone: VibeSkinDecorationTone = "neutral") {
  return (
    <span
      aria-label={label}
      className={`vibe-skin-rescue-dog vibe-skin-rescue-dog-${tone}`}
      role="img"
    >
      <span className="vibe-skin-rescue-dog-ear vibe-skin-rescue-dog-ear-left" />
      <span className="vibe-skin-rescue-dog-ear vibe-skin-rescue-dog-ear-right" />
      <span className="vibe-skin-rescue-dog-face" />
    </span>
  );
}

function renderSkinTemplateFigure(
  template: VibeSkinDecorationTemplate | undefined,
  label: string,
  className = "",
): ReactNode {
  if (template === "qq-mascot") {
    return (
      <div
        aria-label={label}
        className={`vibe-skin-showcase-figure vibe-skin-qq-mascot ${className}`}
        data-testid="vibe-skin-qq-mascot"
        role="img"
      >
        <span className="vibe-skin-qq-mascot-antenna" />
        <span className="vibe-skin-qq-mascot-ear vibe-skin-qq-mascot-ear-left" />
        <span className="vibe-skin-qq-mascot-ear vibe-skin-qq-mascot-ear-right" />
        <span className="vibe-skin-qq-mascot-screen">AI</span>
        <span className="vibe-skin-qq-mascot-scarf" />
      </div>
    );
  }

  if (template === "qq-person") {
    return (
      <div
        aria-label={label}
        className={`vibe-skin-qq-person ${className}`}
        data-testid="vibe-skin-qq-person"
        role="img"
      >
        <span className="vibe-skin-qq-person-hair" />
        <span className="vibe-skin-qq-person-face" />
        <span className="vibe-skin-qq-person-body" />
        <span className="vibe-skin-qq-person-hand vibe-skin-qq-person-hand-left" />
        <span className="vibe-skin-qq-person-hand vibe-skin-qq-person-hand-right" />
      </div>
    );
  }

  if (template === "rescue-rider") {
    return (
      <div
        aria-label={label}
        className={`vibe-skin-rescue-avatar-mark ${className}`}
        data-testid="vibe-skin-rescue-avatar"
        role="img"
      >
        <span className="vibe-skin-rescue-avatar-face" />
        <span className="vibe-skin-rescue-avatar-hair" />
        <span className="vibe-skin-rescue-avatar-vest" />
      </div>
    );
  }

  if (template === "rescue-hq") {
    return (
      <div
        aria-label={label}
        className={`vibe-skin-showcase-figure vibe-skin-rescue-hq ${className}`}
        data-testid="vibe-skin-rescue-hq"
        role="img"
      >
        <span className="vibe-skin-rescue-hq-sky" />
        <span className="vibe-skin-rescue-hq-antenna" />
        <span className="vibe-skin-rescue-hq-deck" />
        <span className="vibe-skin-rescue-hq-window vibe-skin-rescue-hq-window-left" />
        <span className="vibe-skin-rescue-hq-window vibe-skin-rescue-hq-window-right" />
        <span className="vibe-skin-rescue-hq-tower" />
        <span className="vibe-skin-rescue-hq-badge">总部</span>
        <span className="vibe-skin-rescue-hq-base" />
        <span className="vibe-skin-rescue-hq-hill vibe-skin-rescue-hq-hill-left" />
        <span className="vibe-skin-rescue-hq-hill vibe-skin-rescue-hq-hill-right" />
      </div>
    );
  }

  if (template === "rescue-mayor") {
    return (
      <div
        aria-label={label}
        className={`vibe-skin-rescue-mayor ${className}`}
        data-testid="vibe-skin-rescue-mayor"
        role="img"
      >
        <span className="vibe-skin-rescue-mayor-hat" />
        <span className="vibe-skin-rescue-mayor-head" />
        <span className="vibe-skin-rescue-mayor-body" />
      </div>
    );
  }

  if (template === "rescue-chicken") {
    return (
      <div
        aria-label={label}
        className={`vibe-skin-rescue-chicken ${className}`}
        data-testid="vibe-skin-rescue-chicken"
        role="img"
      >
        <span className="vibe-skin-rescue-chicken-comb" />
        <span className="vibe-skin-rescue-chicken-body" />
        <span className="vibe-skin-rescue-chicken-wing" />
      </div>
    );
  }

  if (template === "space-ai-core") {
    return (
      <div
        aria-label={label}
        className={`vibe-skin-space-ai-core ${className}`}
        data-testid="vibe-skin-space-ai-core"
        role="img"
      >
        <span className="vibe-skin-space-ai-core-ring" />
        <span className="vibe-skin-space-ai-core-eye" />
        <span className="vibe-skin-space-ai-core-pulse" />
      </div>
    );
  }

  if (template === "space-ship") {
    return <StarshipHologram className={className} label={label} />;
  }

  if (template === "space-radar") {
    return (
      <div
        aria-label={label}
        className={`vibe-skin-space-radar ${className}`}
        data-testid="vibe-skin-space-radar"
        role="img"
      >
        <span className="vibe-skin-space-radar-grid" />
        <span className="vibe-skin-space-radar-sweep" />
        <span className="vibe-skin-space-radar-blip vibe-skin-space-radar-blip-a" />
        <span className="vibe-skin-space-radar-blip vibe-skin-space-radar-blip-b" />
        <span className="vibe-skin-space-radar-blip vibe-skin-space-radar-blip-c" />
      </div>
    );
  }

  if (template === "space-starmap") {
    return (
      <div
        aria-label={label}
        className={`vibe-skin-space-starmap ${className}`}
        data-testid="vibe-skin-space-starmap"
        role="img"
      >
        <span className="vibe-skin-space-starmap-orbit vibe-skin-space-starmap-orbit-a" />
        <span className="vibe-skin-space-starmap-orbit vibe-skin-space-starmap-orbit-b" />
        <span className="vibe-skin-space-starmap-route" />
        <span className="vibe-skin-space-starmap-node vibe-skin-space-starmap-node-a" />
        <span className="vibe-skin-space-starmap-node vibe-skin-space-starmap-node-b" />
        <span className="vibe-skin-space-starmap-node vibe-skin-space-starmap-node-c" />
      </div>
    );
  }

  return null;
}

function renderSkinDecorationItemFigure(item: VibeSkinDecorationItem) {
  if (item.image) {
    return (
      <img
        alt={`${item.label} image`}
        className="vibe-skin-decoration-image max-h-28 w-full object-contain"
        src={item.image}
      />
    );
  }

  if (item.template) {
    return renderSkinTemplateFigure(item.template, item.label);
  }

  return null;
}

function SkinDecorationCard({
  card,
  regionKeys,
}: {
  card: VibeSkinDecorationCard;
  regionKeys: string[];
}) {
  if (card.template === "qq-person") {
    const friend = card.items?.[0];
    return (
      <div className="vibe-skin-right-card vibe-skin-qq-friend-card mt-3 overflow-hidden rounded-2xl border">
        <div className="vibe-skin-qq-card-title flex items-center justify-between px-3 py-2 text-[12px] font-semibold">
          <span>{card.title ?? "我的好友"}</span>
          <span>{card.badge ?? "QQ秀"}</span>
        </div>
        <div className="vibe-skin-qq-friend-stage mx-3 mt-3 grid place-items-center rounded-2xl border p-3">
          {friend?.image ? (
            <img
              alt={`${friend.label} image`}
              className="vibe-skin-decoration-image max-h-32 w-full object-contain"
              src={friend.image}
            />
          ) : (
            renderSkinTemplateFigure(friend?.template ?? "qq-person", friend?.label ?? "QQ秀好友形象")
          )}
        </div>
        <div className="flex items-center justify-between px-3 py-3 text-[12px]">
          <span className="font-semibold text-[var(--vibe-text)]">{friend?.label ?? "小希"}</span>
          <span className="rounded-full border px-2 py-0.5 text-[11px] text-[var(--vibe-muted-text)]">
            {friend?.badge ?? "在线"}
          </span>
        </div>
      </div>
    );
  }

  if (card.template === "rescue-dog-team") {
    return (
      <div
        className="vibe-skin-right-card vibe-skin-rescue-team-card mt-3 rounded-2xl border p-3"
        data-testid="vibe-skin-rescue-dogs"
      >
        <div className="flex items-center justify-between gap-2">
          <p className="text-[10px] font-semibold tracking-[0.18em] text-[var(--vibe-muted-text)]">
            {card.title ?? "汪汪队员"}
          </p>
          {card.badge && (
            <span className="rounded-full border px-2 py-0.5 text-[10px] font-semibold">
              {card.badge}
            </span>
          )}
        </div>
        <div className="mt-3 grid grid-cols-3 gap-2">
          {(card.items ?? []).map((item) => (
            <span className="grid place-items-center" key={`${item.label}-${item.tone ?? "neutral"}`}>
              {item.image ? (
                <img
                  alt={`${item.label} image`}
                  className="vibe-skin-decoration-image h-12 w-12 object-contain"
                  src={item.image}
                />
              ) : (
                renderRescueDog(item.label, item.tone)
              )}
            </span>
          ))}
        </div>
      </div>
    );
  }

  if (card.template === "rescue-civic") {
    return (
      <div className="vibe-skin-right-card vibe-skin-rescue-civic-card mt-3 rounded-2xl border p-3">
        <p className="text-[10px] font-semibold tracking-[0.18em] text-[var(--vibe-muted-text)]">
          {card.title ?? "冒险湾市政"}
        </p>
        <div className="vibe-skin-rescue-civic-stage mt-3 grid grid-cols-2 gap-2 rounded-2xl border p-2">
          {(card.items ?? []).map((item) => (
            <div className="grid place-items-center gap-1" key={item.label}>
              {renderSkinDecorationItemFigure(item)}
              <span className="text-[11px] font-semibold">{item.label}</span>
            </div>
          ))}
        </div>
      </div>
    );
  }

  if (
    card.template === "space-radar" ||
    card.template === "space-ship" ||
    card.template === "space-starmap"
  ) {
    const templateFigure = renderSkinTemplateFigure(
      card.template,
      card.title || card.badge || "星舰展示",
      "mx-auto",
    );

    return (
      <div className="vibe-skin-right-card vibe-skin-space-card mt-3 rounded-2xl border p-3">
        <div className="flex items-start justify-between gap-2">
          <div className="min-w-0">
            <p className="text-[10px] font-semibold tracking-[0.18em] text-[var(--vibe-muted-text)]">
              {card.badge ?? "HUD"}
            </p>
            {card.title && (
              <h3 className="mt-1 truncate text-[13px] font-semibold text-[var(--vibe-text)]">
                {card.title}
              </h3>
            )}
            {card.subtitle && (
              <p className="mt-1 text-[11px] text-[var(--vibe-muted-text)]">{card.subtitle}</p>
            )}
          </div>
          <span className="vibe-skin-space-led shrink-0 rounded-full border px-2 py-0.5 text-[10px]">
            在线
          </span>
        </div>
        <div className="mt-3 grid place-items-center">{templateFigure}</div>
        {card.items && card.items.length > 0 && (
          <div className="mt-3 grid gap-1.5">
            {card.items.slice(0, 4).map((item) => (
              <p
                className="flex items-center justify-between gap-2 rounded-full border px-2 py-1 text-[11px]"
                key={item.label}
              >
                <span>{item.label}</span>
                {item.badge && <span className="text-[var(--vibe-accent)]">{item.badge}</span>}
              </p>
            ))}
          </div>
        )}
        {card.footer && (
          <p className="mt-3 text-[11px] text-[var(--vibe-muted-text)]">{card.footer}</p>
        )}
      </div>
    );
  }

  if (card.template === "space-telemetry") {
    const telemetryItems = card.items?.length
      ? card.items
      : [
          { label: "跃迁核心", badge: "稳定" },
          { label: "护盾矩阵", badge: "97%" },
          { label: "导航星图", badge: "同步" },
        ];

    return (
      <div
        className="vibe-skin-right-card vibe-skin-space-card vibe-skin-space-telemetry-card mt-3 rounded-2xl border p-3"
        data-testid="vibe-skin-space-telemetry"
      >
        <div className="flex items-center justify-between gap-2">
          <div className="min-w-0">
            <p className="text-[10px] font-semibold tracking-[0.18em] text-[var(--vibe-muted-text)]">
              {card.badge ?? "LIVE"}
            </p>
            <h3 className="mt-1 truncate text-[13px] font-semibold text-[var(--vibe-text)]">
              {card.title ?? "遥测输出"}
            </h3>
          </div>
          <span className="vibe-skin-space-led shrink-0 rounded-full border px-2 py-0.5 text-[10px]">
            检测
          </span>
        </div>
        {card.subtitle && (
          <p className="mt-1 text-[11px] text-[var(--vibe-muted-text)]">{card.subtitle}</p>
        )}
        <div className="vibe-skin-space-telemetry-lines mt-3 rounded-xl border p-2">
          {telemetryItems.slice(0, 6).map((item, index) => (
            <p className="vibe-skin-space-telemetry-line" key={`${item.label}-${index}`}>
              <span className="text-[var(--vibe-muted-text)]">&gt;</span>
              <span>{item.label}</span>
              <span className="ml-auto text-[var(--vibe-accent)]">{item.badge ?? "OK"}</span>
            </p>
          ))}
        </div>
        {card.footer && (
          <p className="mt-2 text-[11px] text-[var(--vibe-muted-text)]">{card.footer}</p>
        )}
      </div>
    );
  }

  const templateFigure = renderSkinTemplateFigure(
    card.template,
    card.title ?? card.badge ?? "皮肤装饰",
    "mx-auto",
  );

  return (
    <div className="vibe-skin-right-card mt-3 rounded-2xl border p-3">
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0">
          <p className="text-[10px] font-semibold tracking-[0.18em] text-[var(--vibe-muted-text)]">
            {card.badge ?? "皮肤区域"}
          </p>
          {card.title && (
            <h3 className="mt-1 truncate text-[13px] font-semibold text-[var(--vibe-text)]">
              {card.title}
            </h3>
          )}
          {card.subtitle && (
            <p className="mt-1 text-[11px] text-[var(--vibe-muted-text)]">{card.subtitle}</p>
          )}
        </div>
      </div>
      {card.figure ? (
        <img
          alt={`${card.title ?? "skin decoration"} figure`}
          className="vibe-skin-decoration-image mx-auto mt-3 max-h-36 w-full object-contain"
          src={card.figure}
        />
      ) : (
        templateFigure && <div className="mt-3 grid place-items-center">{templateFigure}</div>
      )}
      {card.items && card.items.length > 0 ? (
        <div className="mt-3 flex flex-wrap gap-1.5">
          {card.items.map((item) => (
            <span key={item.label} className="rounded-full border px-2 py-1 text-[11px]">
              {item.label}
            </span>
          ))}
        </div>
      ) : (
        <div className="mt-3 flex flex-wrap gap-1.5">
          {regionKeys.length > 0 ? (
            regionKeys.slice(0, 8).map((region) => (
              <span key={region} className="rounded-full border px-2 py-1 text-[11px]">
                {region}
              </span>
            ))
          ) : (
            <span className="rounded-full border px-2 py-1 text-[11px]">ui</span>
          )}
        </div>
      )}
      {card.footer && (
        <p className="mt-3 text-[11px] text-[var(--vibe-muted-text)]">{card.footer}</p>
      )}
    </div>
  );
}

export function VibeScreen({ onExitVibe }: VibeScreenProps) {
  const { t } = useI18n();
  const [tabs, setTabs] = useState<TerminalSession[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [createDialogOpen, setCreateDialogOpen] = useState(false);
  const [createProjectDir, setCreateProjectDir] = useState("");
  const [createPlatform, setCreatePlatform] = useState<AgentPlatform>("codex");
  const [launchPrompt, setLaunchPrompt] = useState("");
  const [launchModel, setLaunchModel] =
    useState<(typeof launchModelOptions)[number]["value"]>("auto");
  const [launchReasoning, setLaunchReasoning] =
    useState<(typeof launchReasoningOptions)[number]["value"]>("auto");
  const [themeMode, setThemeMode] = useState<VibeTheme>("dark");
  const [appearanceOpen, setAppearanceOpen] = useState(false);
  const [startMenuOpen, setStartMenuOpen] = useState(false);
  const [clockNow, setClockNow] = useState(() => new Date());
  const [customSkin, setCustomSkin] = useState<VibeSkinDefinition | null>(() => readStoredVibeSkin());
  const [activeSkinId, setActiveSkinId] = useState<string>(
    () => readStoredVibeSkin()?.id ?? BUILT_IN_VIBE_SKINS[0].id,
  );
  const [error, setError] = useState<string | null>(null);
  const [sessionListScrolling, setSessionListScrolling] = useState(false);
  const [expandedDirectories, setExpandedDirectories] = useState<Set<string>>(() => new Set());
  const skinFileInputRef = useRef<HTMLInputElement | null>(null);
  const startButtonRef = useRef<HTMLButtonElement | null>(null);
  const startMenuRef = useRef<HTMLDivElement | null>(null);
  const sessionListScrollTimeout = useRef<number | null>(null);

  const sessionsQuery = useQuery({
    queryKey: ["sessions"],
    queryFn: () => listSessions(null),
  });

  const groupedSessions = useMemo(
    () => groupSessions(sessionsQuery.data ?? [], t("vibe.unknownDirectory")),
    [sessionsQuery.data, t],
  );
  const projectDirectories = useMemo(() => {
    const directories = new Set<string>();
    for (const session of sessionsQuery.data ?? []) {
      const directory = session.projectDir?.trim();
      if (directory) {
        directories.add(directory);
      }
    }
    return Array.from(directories);
  }, [sessionsQuery.data]);

  useEffect(() => {
    setCreateProjectDir((current) => current || projectDirectories[0] || "");
  }, [projectDirectories]);

  const availableSkins = useMemo(
    () => [...BUILT_IN_VIBE_SKINS, ...(customSkin ? [customSkin] : [])],
    [customSkin],
  );
  const activeSkin = useMemo(
    () => availableSkins.find((skin) => skin.id === activeSkinId) ?? BUILT_IN_VIBE_SKINS[0],
    [activeSkinId, availableSkins],
  );

  const openTerminal = useCallback(async (input: CreateTerminalSessionInput) => {
    setError(null);
    try {
      const session = await createTerminalSession(input);
      setTabs((current) => [...current, session]);
      setActiveId(session.id);
    } catch (caught) {
      setError(formatError(caught));
    }
  }, []);

  const resumeSession = (session: SessionMeta) => {
    if (!session.projectDir || !session.resumeCommand) {
      setError(t("vibe.errorMissingSession"));
      return;
    }

    void openTerminal({
      kind: "resume",
      platform: session.providerId,
      command: session.resumeCommand,
      title: titleForSession(session, t("vibe.unknownDirectory")),
      cwd: session.projectDir,
      cols: 100,
      rows: 30,
    });
  };

  const openCreateDialog = () => {
    setCreateProjectDir((current) => current || projectDirectories[0] || "");
    setCreateDialogOpen(true);
  };

  const chooseFolder = async () => {
    const selected = await open({
      directory: true,
      multiple: false,
      title: t("vibe.chooseFolder"),
    });
    if (typeof selected === "string") {
      setCreateProjectDir(selected);
    }
  };

  const handleLaunchFolderChange = (event: ChangeEvent<HTMLSelectElement>) => {
    const value = event.target.value;
    if (value === chooseFolderOptionValue) {
      void chooseFolder();
      return;
    }
    setCreateProjectDir(value);
  };

  const importSkin = async (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    event.target.value = "";
    if (!file) {
      return;
    }

    setError(null);
    try {
      const skin = await importVibeSkinPackage(file);
      writeStoredVibeSkin(skin);
      setCustomSkin(skin);
      setActiveSkinId(skin.id);
      setThemeMode("skin");
    } catch (caught) {
      setError(formatError(caught));
    }
  };

  const clearCustomSkin = () => {
    clearStoredVibeSkin();
    setCustomSkin(null);
    if (activeSkinId === customSkin?.id) {
      setActiveSkinId(BUILT_IN_VIBE_SKINS[0].id);
    }
    setThemeMode("skin");
  };

  const launchAgentSession = (closeDialog: boolean) => {
    const cwd = createProjectDir.trim();
    if (!cwd) {
      setError(t("vibe.errorProjectRequired"));
      return;
    }

    void openTerminal({
      kind: "agent",
      platform: createPlatform,
      command: null,
      title: `${createPlatform} - ${cwd}`,
      cwd,
      cols: 100,
      rows: 30,
    });
    setLaunchPrompt("");
    if (closeDialog) {
      setCreateDialogOpen(false);
    }
  };

  const launchNewAgent = () => {
    launchAgentSession(true);
  };

  const launchFromComposer = () => {
    launchAgentSession(false);
  };

  const closeTab = async (session: TerminalSession) => {
    setError(null);
    try {
      if (session.status === "running") {
        await killTerminalSession(session.id);
      }
      setTabs((current) => {
        const remaining = current.filter((tab) => tab.id !== session.id);
        setActiveId((currentActive) => {
          if (currentActive !== session.id) {
            return currentActive;
          }
          return remaining[0]?.id ?? null;
        });
        return remaining;
      });
    } catch (caught) {
      setError(formatError(caught));
    }
  };

  const updateStatus = useCallback((sessionId: string, status: TerminalStatus) => {
    setTabs((current) =>
      current.map((tab) => (tab.id === sessionId ? { ...tab, status } : tab)),
    );
  }, []);

  const toggleDirectory = useCallback((directory: string) => {
    setExpandedDirectories((current) => {
      const next = new Set(current);
      if (next.has(directory)) {
        next.delete(directory);
      } else {
        next.add(directory);
      }
      return next;
    });
  }, []);

  const markSessionListScrolling = useCallback(() => {
    if (sessionListScrollTimeout.current !== null) {
      window.clearTimeout(sessionListScrollTimeout.current);
    }

    setSessionListScrolling(true);
    sessionListScrollTimeout.current = window.setTimeout(() => {
      setSessionListScrolling(false);
      sessionListScrollTimeout.current = null;
    }, 800);
  }, []);

  useEffect(() => {
    return () => {
      if (sessionListScrollTimeout.current !== null) {
        window.clearTimeout(sessionListScrollTimeout.current);
      }
    };
  }, []);

  const activeTab = tabs.find((tab) => tab.id === activeId) ?? null;
  const isDark = themeMode === "dark";
  const isSkin = themeMode === "skin";
  const decorations = activeSkin.decorations;
  const skinVariant = skinVariantClass(isSkin ? decorations?.variant : undefined);
  const isStarshipSkin = Boolean(isSkin && decorations?.variant === "starship-cockpit");
  const skinStyle = useMemo(
    () => (isSkin ? skinToCssVariables(activeSkin) : undefined),
    [activeSkin, isSkin],
  );
  const terminalThemeMode = isDark ? "dark" : "light";
  const themeLabel =
    themeMode === "dark"
      ? t("vibe.themeDark")
      : themeMode === "light"
        ? t("vibe.themeLight")
        : t("vibe.themeSkin");
  const scrollbarThemeClass = isSkin
    ? "vibe-scrollbar-skin"
    : isDark
      ? "vibe-scrollbar-dark"
      : "vibe-scrollbar-light";
  const skinBlocks = useMemo(() => getVibeSkinBlocks(activeSkin), [activeSkin]);
  const skinRightCards = decorations?.rightCards ?? [];
  const showSkinRightRail = Boolean(
    isSkin && (skinBlocks.showcase.enabled || skinRightCards.length > 0),
  );
  const skinBodyGridClass = showSkinRightRail
    ? "vibe-skin-body grid min-h-0 flex-1 grid-cols-1 lg:grid-cols-[300px_minmax(0,1fr)_260px]"
    : "vibe-skin-body grid min-h-0 flex-1 grid-cols-1 lg:grid-cols-[300px_minmax(0,1fr)]";
  const activeSkinRegionKeys = isSkin
    ? VIBE_SKIN_REGION_KEYS.filter((region) => Boolean(activeSkin.regions?.[region]))
    : [];
  const taskbarEnabled = Boolean(isSkin && skinBlocks.taskbar.enabled);
  const currentTime = formatTaskbarClock(clockNow);

  useEffect(() => {
    if (!taskbarEnabled) {
      return;
    }

    const interval = window.setInterval(() => setClockNow(new Date()), 30_000);
    return () => window.clearInterval(interval);
  }, [taskbarEnabled]);

  useEffect(() => {
    if (!startMenuOpen) {
      return;
    }

    const closeOnOutsideMouseDown = (event: MouseEvent) => {
      const target = event.target;
      if (!(target instanceof Node)) {
        return;
      }
      if (startButtonRef.current?.contains(target) || startMenuRef.current?.contains(target)) {
        return;
      }
      setStartMenuOpen(false);
    };

    window.addEventListener("mousedown", closeOnOutsideMouseDown);
    return () => window.removeEventListener("mousedown", closeOnOutsideMouseDown);
  }, [startMenuOpen]);

  useEffect(() => {
    if (!taskbarEnabled) {
      setStartMenuOpen(false);
    }
  }, [taskbarEnabled]);

  const openAppearance = () => {
    setStartMenuOpen(false);
    setAppearanceOpen(true);
  };

  const triggerSkinImport = () => {
    setStartMenuOpen(false);
    skinFileInputRef.current?.click();
  };

  const runTaskbarMenuItem = (item: VibeSkinTaskbarMenuItem) => {
    if ("type" in item || item.disabled || !item.action) {
      return;
    }

    setStartMenuOpen(false);
    if (item.action === "openAppearance") {
      setAppearanceOpen(true);
      return;
    }
    if (item.action === "setTheme") {
      if (item.theme === "dark" || item.theme === "light" || item.theme === "skin") {
        setThemeMode(item.theme);
      }
      return;
    }
    if (item.action === "importSkin") {
      skinFileInputRef.current?.click();
      return;
    }
    if (item.action === "clearSkin" && customSkin) {
      clearCustomSkin();
    }
  };

  const launchTitle = isSkin ? skinBlocks.launch.title : t("vibe.emptyTitle");
  const launchBody = isSkin ? skinBlocks.launch.body : t("vibe.emptyBody");
  const launchPlaceholder = isSkin
    ? skinBlocks.launch.placeholder
    : t("vibe.launchPlaceholder");
  const launchSendLabel = isSkin ? skinBlocks.launch.sendLabel : t("vibe.launchSend");
  const launchFolderLabel = isSkin ? skinBlocks.launch.folderLabel : t("vibe.launchFolder");
  const launchModelLabel = isSkin ? skinBlocks.launch.modelLabel : t("vibe.launchModel");
  const launchReasoningLabel = isSkin
    ? skinBlocks.launch.reasoningLabel
    : t("vibe.launchReasoning");
  const launchAgentStripLabel = isSkin
    ? skinBlocks.launch.agentStripLabel
    : t("vibe.launchAgentFullAccess");
  const launchAgentPrefix = isSkin ? skinBlocks.launch.agentStripPrefix : "";
  const launchAgentSuffix = isSkin ? skinBlocks.launch.agentStripSuffix : "";
  const launchExtraLabel = isSkin ? skinBlocks.launch.extraLabel : "";
  const launchExtraValue = isSkin ? skinBlocks.launch.extraValue : "";
  const customProjectDirectory =
    createProjectDir.trim() && !projectDirectories.includes(createProjectDir.trim())
      ? createProjectDir.trim()
      : null;
  const launchPanelClass = isSkin
    ? "vibe-skin-launch-panel mx-auto w-full max-w-4xl shrink-0 rounded-[1.5rem] border p-3 text-left shadow-2xl backdrop-blur-xl sm:p-4"
    : isDark
      ? "mx-auto w-full max-w-4xl shrink-0 rounded-[1.5rem] border border-[#073642] bg-[#073642]/70 p-3 text-left shadow-2xl shadow-black/30 backdrop-blur-xl sm:p-4"
      : "mx-auto w-full max-w-4xl shrink-0 rounded-[1.5rem] border border-white/80 bg-white/82 p-3 text-left shadow-2xl shadow-stone-900/10 backdrop-blur-xl sm:p-4";
  const agentStripClass = isSkin
    ? "vibe-skin-agent-strip rounded-2xl border p-2"
    : isDark
      ? "rounded-2xl border border-[#586e75]/55 bg-[#002b36]/72 p-2 text-[#d8e2dc]"
      : "rounded-2xl border border-stone-200 bg-white/78 p-2 text-stone-700";
  const agentOptionClass = (active: boolean) =>
    isSkin
      ? `vibe-skin-agent-option ${
          active ? "vibe-skin-agent-option-active" : ""
        } inline-flex items-center gap-2 rounded-xl border px-3 py-2 text-[12px] font-semibold transition`
      : isDark
        ? `inline-flex items-center gap-2 rounded-xl border px-3 py-2 text-[12px] font-semibold transition ${
            active
              ? "border-[#2aa198] bg-[#2aa198]/18 text-[#fdf6e3] shadow-[0_0_18px_rgba(42,161,152,0.18)]"
              : "border-[#586e75]/50 bg-[#073642]/64 text-[#9fc3cf] hover:border-[#839496] hover:text-[#fdf6e3]"
          }`
        : `inline-flex items-center gap-2 rounded-xl border px-3 py-2 text-[12px] font-semibold transition ${
            active
              ? "border-emerald-300 bg-emerald-50 text-emerald-900 shadow-sm"
              : "border-stone-200 bg-white/70 text-stone-600 hover:border-stone-300 hover:text-stone-950"
          }`;
  const composerClass = isSkin
    ? "vibe-skin-composer mt-3 rounded-2xl border p-2"
    : isDark
      ? "mt-3 rounded-2xl border border-[#586e75]/50 bg-[#001e27]/72 p-2"
      : "mt-3 rounded-2xl border border-stone-200 bg-stone-50/82 p-2";
  const composerInputClass = isSkin
    ? "vibe-skin-composer-input min-h-20 w-full resize-none rounded-xl border px-3 py-3 text-sm outline-none transition"
    : isDark
      ? "min-h-20 w-full resize-none rounded-xl border border-[#586e75]/50 bg-[#002b36]/70 px-3 py-3 text-sm text-[#fdf6e3] outline-none placeholder:text-[#586e75] focus:border-[#268bd2]"
      : "min-h-20 w-full resize-none rounded-xl border border-stone-200 bg-white px-3 py-3 text-sm text-stone-950 outline-none placeholder:text-stone-400 focus:border-blue-400";
  const composerMetaBarClass = isSkin
    ? "vibe-skin-composer-meta-bar mt-2 flex flex-col gap-2 rounded-xl border p-2 sm:flex-row sm:items-end"
    : isDark
      ? "mt-2 flex flex-col gap-2 rounded-xl border border-[#586e75]/40 bg-[#073642]/50 p-2 sm:flex-row sm:items-end"
      : "mt-2 flex flex-col gap-2 rounded-xl border border-stone-200 bg-white/70 p-2 sm:flex-row sm:items-end";
  const composerControlClass = isSkin
    ? "vibe-skin-composer-control mt-1 h-10 w-full min-w-0 rounded-xl border px-3 text-[12px] outline-none transition"
    : isDark
      ? "mt-1 h-10 w-full min-w-0 rounded-xl border border-[#586e75]/55 bg-[#002b36] px-3 text-[12px] text-[#fdf6e3] outline-none focus:border-[#268bd2]"
      : "mt-1 h-10 w-full min-w-0 rounded-xl border border-stone-200 bg-white px-3 text-[12px] text-stone-950 outline-none focus:border-blue-400";
  const composerSendButtonClass = isSkin
    ? "vibe-skin-composer-send-button inline-flex h-10 shrink-0 items-center justify-center gap-2 rounded-xl border px-4 text-[13px] font-semibold transition sm:ml-auto"
    : isDark
      ? "inline-flex h-10 shrink-0 items-center justify-center gap-2 rounded-xl border border-[#b58900] bg-[#b58900] px-4 text-[13px] font-semibold text-[#002b36] transition hover:bg-[#cb4b16] hover:text-white sm:ml-auto"
      : "inline-flex h-10 shrink-0 items-center justify-center gap-2 rounded-xl bg-stone-950 px-4 text-[13px] font-semibold text-white transition hover:bg-stone-800 sm:ml-auto";
  const composerAddonClass = isSkin
    ? "vibe-skin-composer-addon inline-flex shrink-0 items-center gap-1 rounded-full border px-3 py-1 text-[11px] font-semibold"
    : isDark
      ? "inline-flex shrink-0 items-center gap-1 rounded-full border border-[#586e75]/50 px-3 py-1 text-[11px] font-semibold text-[#9fc3cf]"
      : "inline-flex shrink-0 items-center gap-1 rounded-full border border-stone-200 px-3 py-1 text-[11px] font-semibold text-stone-500";

  return (
    <main
      className={
        isSkin
          ? `vibe-skin ${skinVariant} h-screen max-h-[100dvh] overflow-hidden text-[var(--vibe-text)]`
          : isDark
            ? "h-screen max-h-[100dvh] overflow-hidden bg-[#002b36] text-[#d8e2dc]"
            : "h-screen max-h-[100dvh] overflow-hidden text-stone-950"
      }
      style={skinStyle}
    >
      <div className={isSkin ? "vibe-skin-frame flex h-full min-h-0 flex-col" : "grid h-full min-h-0 grid-cols-1 lg:grid-cols-[356px_minmax(0,1fr)]"}>
        {isSkin && (
          <div className="vibe-skin-titlebar flex h-11 shrink-0 items-center justify-between gap-3 border-b px-3 text-[11px] font-semibold">
            <div className="flex min-w-0 flex-1 items-center gap-2 overflow-hidden">
              <span className="grid h-5 w-5 shrink-0 place-items-center rounded-full border border-[rgba(255,255,255,0.65)] bg-[var(--vibe-accent)] text-[10px] text-white shadow-[inset_0_1px_0_rgba(255,255,255,0.72)]">
                {decorations?.titlebarMark ?? "V"}
              </span>
              <div className="min-w-0 flex-1 overflow-hidden">
                <p
                  className="flex min-w-0 items-center gap-2 overflow-hidden whitespace-nowrap text-[13px] tracking-normal"
                  title={`${skinBlocks.titlebar.title} · ${skinBlocks.titlebar.subtitle}`}
                >
                  <span className="min-w-0 truncate">{skinBlocks.titlebar.title}</span>
                  <span className="shrink-0 text-[10px] tracking-[0.12em] opacity-85">
                    {skinBlocks.titlebar.subtitle}
                  </span>
                </p>
              </div>
            </div>
            <div className="flex shrink-0 items-center gap-2">
              <span className="rounded-full border border-[rgba(255,255,255,0.48)] px-2 py-1 text-[10px] tracking-[0.12em]">
                {skinBlocks.titlebar.badge}
              </span>
              <div
                aria-hidden="true"
                className="vibe-skin-titlebar-controls flex items-center gap-1"
                data-testid="vibe-window-controls"
              >
                <span className="vibe-skin-window-button vibe-skin-window-button-minimize">—</span>
                <span className="vibe-skin-window-button vibe-skin-window-button-maximize">□</span>
                <span className="vibe-skin-window-button vibe-skin-window-button-close">×</span>
              </div>
            </div>
          </div>
        )}
        <div className={isSkin ? skinBodyGridClass : "contents"}>
          {isStarshipSkin && (
            <div
              aria-hidden="true"
              className="vibe-skin-space-planets"
              data-testid="vibe-skin-space-planets"
            >
              <span
                className="vibe-skin-space-planet vibe-skin-space-planet-large"
                data-testid="vibe-skin-space-planet"
              />
              <span
                className="vibe-skin-space-planet vibe-skin-space-planet-medium"
                data-testid="vibe-skin-space-planet"
              />
              <span
                className="vibe-skin-space-planet vibe-skin-space-planet-small"
                data-testid="vibe-skin-space-planet"
              />
            </div>
          )}
        <aside
          className={
            isSkin
              ? "vibe-skin-sidebar relative flex h-full min-h-0 flex-col overflow-hidden border-b p-3 shadow-2xl lg:border-b-0 lg:border-r"
              : isDark
              ? "relative flex h-full min-h-0 flex-col overflow-hidden border-b border-[#073642] bg-[#002b36] p-3 shadow-2xl shadow-black/25 lg:border-b-0 lg:border-r lg:border-[#073642]"
              : "relative flex h-full min-h-0 flex-col overflow-hidden border-b border-white/70 bg-gradient-to-br from-slate-50/92 via-emerald-50/74 to-amber-50/70 p-3 shadow-xl shadow-stone-900/5 backdrop-blur-2xl lg:border-b-0 lg:border-r lg:border-white/80"
          }
        >
          <div
            className={
              isSkin
                ? "vibe-skin-backdrop pointer-events-none absolute inset-0"
                : isDark
                ? "pointer-events-none absolute inset-0 bg-[radial-gradient(circle_at_18%_12%,rgba(38,139,210,0.18),transparent_30%),radial-gradient(circle_at_90%_10%,rgba(181,137,0,0.18),transparent_28%),linear-gradient(180deg,rgba(7,54,66,0.78),rgba(0,43,54,0.92))]"
                : "pointer-events-none absolute inset-0 bg-[radial-gradient(circle_at_20%_15%,rgba(16,185,129,0.18),transparent_34%),radial-gradient(circle_at_88%_8%,rgba(245,158,11,0.16),transparent_30%),linear-gradient(180deg,rgba(255,255,255,0.72),rgba(255,255,255,0.38))]"
            }
          />
          <div className="relative flex min-h-0 flex-1 flex-col">
            <div
              className={
                isSkin
                  ? "vibe-skin-sidebar-header mb-4 flex items-start justify-between gap-3 rounded-2xl border p-3 shadow-sm"
                  : isDark
                  ? "mb-4 flex items-start justify-between gap-3 rounded-2xl border border-[#073642] bg-[#073642]/65 p-3 shadow-sm backdrop-blur-xl"
                  : "mb-4 flex items-start justify-between gap-3 rounded-2xl border border-white/80 bg-white/56 p-3 shadow-sm backdrop-blur-xl"
              }
            >
              {isSkin ? (
                <>
                  <div className="vibe-skin-profile flex min-w-0 flex-1 items-center gap-3">
                    <div className="vibe-skin-avatar relative grid h-14 w-14 shrink-0 place-items-center overflow-hidden rounded-2xl border">
                      {skinBlocks.profile.avatar ? (
                        <img
                          alt={`${skinBlocks.profile.name} avatar`}
                          className="h-full w-full object-cover"
                          src={skinBlocks.profile.avatar}
                        />
                      ) : decorations?.avatarTemplate ? (
                        renderSkinTemplateFigure(decorations.avatarTemplate, "莱德队长头像")
                      ) : (
                        <AiSwitchLogo className="h-9 w-9 rounded-xl" />
                      )}
                      <span className="vibe-skin-online-badge absolute bottom-1 right-1 h-3.5 w-3.5 rounded-full border-2" />
                    </div>
                    <div className="min-w-0">
                      <div className="flex min-w-0 items-center gap-2">
                        <h1 className="truncate text-[14px] font-semibold text-[var(--vibe-text)]">
                          {skinBlocks.profile.name}
                        </h1>
                        <span className="vibe-skin-profile-badge rounded-full border px-2 py-0.5 text-[10px]">
                          {skinBlocks.profile.badge}
                        </span>
                      </div>
                      <p className="mt-0.5 truncate text-[11px] text-[var(--vibe-muted-text)]">
                        {skinBlocks.profile.status}
                      </p>
                      <p className="mt-1 truncate text-[11px] text-[var(--vibe-text)] opacity-80">
                        {skinBlocks.profile.signature}
                      </p>
                    </div>
                  </div>
                  <button
                    aria-label={t("layout.switchToAgent")}
                    className="vibe-skin-ghost grid h-8 w-8 shrink-0 place-items-center rounded-xl border shadow-sm transition-colors focus:outline-none focus-visible:ring-2"
                    onClick={onExitVibe}
                    type="button"
                  >
                    <PanelLeftClose className="h-4 w-4" />
                  </button>
                </>
              ) : (
                <>
                  <div className="flex min-w-0 items-center gap-2">
                    <AiSwitchLogo className="h-9 w-9 shrink-0 rounded-2xl shadow-sm" />
                    <div className="min-w-0">
                      <h1 className={isDark ? "truncate text-[13px] font-semibold text-[#fdf6e3]" : "truncate text-[13px] font-semibold text-stone-950"}>
                        {t("vibe.title")} · {t("vibe.kicker")}
                      </h1>
                      <p className={isDark ? "truncate text-[11px] text-[#93a1a1]" : "truncate text-[11px] text-stone-500"}>
                        {t("vibe.subtitle")}
                      </p>
                    </div>
                  </div>
                  <button
                    aria-label={t("layout.switchToAgent")}
                    className={
                      isDark
                        ? "grid h-8 w-8 shrink-0 place-items-center rounded-xl border border-[#586e75] bg-[#073642] text-[#93a1a1] shadow-sm transition-colors hover:border-[#839496] hover:text-[#fdf6e3] focus:outline-none focus-visible:ring-2 focus-visible:ring-[#268bd2]"
                        : "grid h-8 w-8 shrink-0 place-items-center rounded-xl border border-stone-200 bg-white/70 text-stone-600 shadow-sm transition-colors hover:border-stone-300 hover:bg-white hover:text-stone-950 focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-400"
                    }
                    onClick={onExitVibe}
                    type="button"
                  >
                    <PanelLeftClose className="h-4 w-4" />
                  </button>
                </>
              )}
            </div>

            <div
              className={
                isSkin
                  ? "vibe-skin-control-panel mb-2 flex flex-wrap items-center gap-2 rounded-2xl border p-3 shadow-sm backdrop-blur-xl"
                  : isDark
                    ? "mb-2 flex items-center gap-2 rounded-2xl border border-[#073642] bg-[#073642]/55 p-3"
                    : "mb-2 flex items-center gap-2 rounded-2xl border border-white/80 bg-white/56 p-3 shadow-sm backdrop-blur-xl"
              }
            >
              <button
                className={
                  isSkin
                    ? "vibe-skin-primary inline-flex flex-1 items-center justify-center gap-2 rounded-xl border px-3 py-2 text-[13px] font-semibold transition"
                    : isDark
                    ? "inline-flex flex-1 items-center justify-center gap-2 rounded-xl border border-[#b58900] bg-[#b58900] px-3 py-2 text-[13px] font-semibold text-[#002b36] transition hover:bg-[#cb4b16] hover:text-white"
                    : "inline-flex flex-1 items-center justify-center gap-2 rounded-xl bg-stone-950 px-3 py-2 text-[13px] font-semibold text-white transition hover:bg-stone-800"
                }
                onClick={openCreateDialog}
                type="button"
              >
                <Plus className="h-4 w-4" />
                {t("vibe.newSession")}
              </button>
              <button
                aria-label={t("vibe.switchTheme")}
                className={
                  isSkin
                    ? "vibe-skin-ghost inline-flex h-9 shrink-0 items-center gap-1.5 rounded-xl border px-2 text-[12px] font-semibold transition-colors"
                    : isDark
                    ? "inline-flex h-9 shrink-0 items-center gap-1.5 rounded-xl border border-[#586e75] bg-[#002b36] px-2 text-[12px] font-semibold text-[#fdf6e3] transition-colors hover:border-[#839496] hover:bg-[#073642]"
                    : "inline-flex h-9 shrink-0 items-center gap-1.5 rounded-xl border border-stone-200 bg-white px-2 text-[12px] font-semibold text-stone-700 transition-colors hover:border-stone-300 hover:bg-stone-50"
                }
                onClick={openAppearance}
                type="button"
              >
                {themeMode === "dark" ? (
                  <MoonStar className="h-4 w-4" />
                ) : themeMode === "light" ? (
                  <SunMedium className="h-4 w-4" />
                ) : (
                  <Palette className="h-4 w-4" />
                )}
                <span>{themeLabel}</span>
              </button>
              <input
                ref={skinFileInputRef}
                aria-label={t("vibe.skinFileInput")}
                className="sr-only"
                type="file"
                accept=".aiskin,.json,.zip,application/json,application/zip"
                onChange={(event) => void importSkin(event)}
              />
            </div>

            {error && (
              <p
                className={
                  isSkin
                    ? "vibe-skin-danger mb-2 rounded-xl border p-2 text-[12px] shadow-lg"
                    : "mb-2 rounded-xl border border-red-400/40 bg-red-950/90 p-2 text-[12px] text-red-100 shadow-lg"
                }
              >
                {error}
              </p>
            )}

            <div
              className={`vibe-scrollbar ${scrollbarThemeClass} ${
                sessionListScrolling ? "vibe-scrollbar-active" : ""
              } ${isSkin ? "vibe-skin-session-list" : isDark ? "vibe-dark-session-list" : "vibe-light-session-list"} min-h-0 flex-1 space-y-3 overflow-y-auto p-3`}
              onScroll={markSessionListScrolling}
            >
              {sessionsQuery.isLoading && (
                <p className={isSkin ? "text-sm text-[var(--vibe-muted-text)]" : isDark ? "text-sm text-[#93a1a1]" : "text-sm text-zinc-400"}>
                  {t("vibe.loadingSessions")}
                </p>
              )}
              {!sessionsQuery.isLoading && groupedSessions.length === 0 && (
                <p
                  className={
                    isSkin
                      ? "vibe-skin-panel rounded-2xl border p-3 text-sm"
                      : isDark
                      ? "rounded-2xl border border-[#073642] bg-[#073642]/55 p-3 text-sm text-[#93a1a1]"
                      : "rounded-2xl border border-stone-200 bg-white/70 p-3 text-sm text-stone-500 shadow-sm"
                  }
                >
                  {t("vibe.noSessions")}
                </p>
              )}
              {groupedSessions.map((group) => {
                const expanded = expandedDirectories.has(group.key);
                const ToggleIcon = expanded ? ChevronDown : ChevronRight;
                return (
                  <div
                    className={
                      isSkin
                        ? "vibe-skin-group-panel rounded-2xl border p-2"
                        : isDark
                        ? "vibe-dark-group-panel rounded-2xl border p-2"
                        : "vibe-light-group-panel rounded-2xl border p-2"
                    }
                    key={group.key}
                  >
                    <button
                      aria-expanded={expanded}
                      aria-label={
                        expanded
                          ? t("vibe.collapseDirectoryAria", { directory: group.title })
                          : t("vibe.expandDirectoryAria", { directory: group.title })
                      }
                      title={group.title}
                      className={
                        isSkin
                          ? "vibe-skin-list-trigger flex w-full items-center gap-2 rounded-xl px-1 py-1 text-left text-[12px] font-semibold transition"
                          : isDark
                            ? "vibe-dark-list-trigger flex w-full items-center gap-2 rounded-xl px-1 py-1 text-left text-[12px] font-semibold transition"
                            : "vibe-light-list-trigger flex w-full items-center gap-2 rounded-xl px-1 py-1 text-left text-[12px] font-semibold transition"
                      }
                      onClick={() => toggleDirectory(group.key)}
                      type="button"
                    >
                      <ToggleIcon className={isSkin ? "h-3.5 w-3.5 shrink-0 text-[var(--vibe-muted-text)]" : isDark ? "h-3.5 w-3.5 shrink-0 text-[#8fb0bc]" : "h-3.5 w-3.5 shrink-0 text-emerald-600/70"} />
                      <FolderOpen className={isSkin ? "h-4 w-4 shrink-0 text-[var(--vibe-accent)]" : isDark ? "h-4 w-4 shrink-0 text-[#38bdf8]" : "h-4 w-4 shrink-0 text-amber-500"} />
                      <span className="truncate">{group.label}</span>
                    </button>
                    {expanded && (
                      <div className="mt-2 space-y-1.5">
                        {group.items.map((session) => {
                          const canResume = Boolean(session.projectDir && session.resumeCommand);
                          const title = titleForSession(session, t("vibe.unknownDirectory"));
                          return (
                            <button
                              aria-label={
                                canResume
                                  ? t("vibe.resumeAria", { title })
                                  : t("vibe.cannotResumeAria", { title })
                              }
                              className={
                                isSkin
                                  ? "vibe-skin-session w-full rounded-xl border px-3 py-2 text-left text-[13px] transition disabled:cursor-not-allowed disabled:opacity-45"
                                  : isDark
                                    ? "vibe-dark-session-card w-full rounded-xl border px-3 py-2 text-left text-[13px] transition disabled:cursor-not-allowed disabled:opacity-55"
                                    : "vibe-light-session-card w-full rounded-xl border px-3 py-2 text-left text-[13px] transition disabled:cursor-not-allowed disabled:opacity-45"
                              }
                              disabled={!canResume}
                              key={sessionKey(session)}
                              onClick={() => resumeSession(session)}
                              type="button"
                            >
                              <span className="flex items-center justify-between gap-2">
                                <span className="truncate font-semibold">{title}</span>
                                <Play className={isSkin ? "h-3.5 w-3.5 shrink-0 text-[var(--vibe-accent)]" : isDark ? "h-3.5 w-3.5 shrink-0 text-[#5eead4]" : "h-3.5 w-3.5 shrink-0 text-emerald-600"} />
                              </span>
                              <span className={isSkin ? "mt-0.5 block truncate text-[11px] text-[var(--vibe-muted-text)]" : isDark ? "vibe-dark-session-meta mt-0.5 block truncate text-[11px]" : "vibe-light-session-meta mt-0.5 block truncate text-[11px]"}>
                                {session.providerId} · {session.resumeCommand ?? t("vibe.missingResumeCommand")}
                              </span>
                            </button>
                          );
                        })}
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          </div>
        </aside>

        <div
          className={
            isSkin
              ? "vibe-skin-workspace flex h-full min-h-0 min-w-0 flex-col overflow-hidden shadow-xl"
              : isDark
                ? "flex h-full min-h-0 min-w-0 flex-col overflow-hidden bg-[#002b36] shadow-xl shadow-black/20"
                : "flex h-full min-h-0 min-w-0 flex-col overflow-hidden bg-slate-50 shadow-xl shadow-stone-900/5"
          }
        >
          <div
            className={
              isSkin
                ? `vibe-scrollbar ${scrollbarThemeClass} vibe-scrollbar-horizontal vibe-skin-tabbar flex h-10 shrink-0 items-stretch gap-0 overflow-x-auto border-b px-1`
                : isDark
                  ? "vibe-scrollbar vibe-scrollbar-dark vibe-scrollbar-horizontal vibe-dark-tabbar flex h-10 shrink-0 items-stretch gap-1 overflow-x-auto border-b px-1"
                  : "vibe-scrollbar vibe-scrollbar-light vibe-scrollbar-horizontal vibe-light-tabbar flex h-10 shrink-0 items-stretch gap-1 overflow-x-auto border-b px-1"
            }
          >
            {tabs.length === 0 && (
              <p className={isSkin ? "flex items-center px-3 text-[12px] text-[var(--vibe-muted-text)]" : isDark ? "flex items-center px-3 text-[12px] text-[#9fc3cf]" : "flex items-center px-3 text-[12px] text-stone-500"}>
                {t("vibe.noTabs")}
              </p>
            )}
            {tabs.map((tab) => (
              <div
                className={`group relative inline-flex h-full max-w-[220px] min-w-[132px] shrink-0 items-center border-r ${
                  activeId === tab.id
                    ? isSkin
                      ? "vibe-skin-tab-active text-[var(--vibe-text)]"
                      : isDark
                        ? "vibe-dark-tab-active"
                        : "vibe-light-tab-active"
                    : isSkin
                      ? "vibe-skin-tab text-[var(--vibe-muted-text)]"
                      : isDark
                        ? "vibe-dark-tab"
                        : "vibe-light-tab"
                }`}
                key={tab.id}
                title={`${tab.title} · ${statusLabel(tab.status, t)}`}
              >
                {activeId === tab.id && (
                  <span
                    className={
                      isSkin
                        ? "absolute inset-x-0 bottom-0 h-[2px] bg-[var(--vibe-accent)]"
                        : isDark
                          ? "absolute inset-x-2 bottom-0 h-[2px] rounded-full bg-[#38bdf8]"
                          : "absolute inset-x-0 bottom-0 h-[2px] bg-amber-400"
                    }
                  />
                )}
                <button
                  className="inline-flex h-full min-w-0 flex-1 items-center gap-2 px-3 pr-1 text-[12px] font-medium"
                  onClick={() => setActiveId(tab.id)}
                  type="button"
                >
                  <span className={`h-1.5 w-1.5 shrink-0 rounded-full ${statusDotClass(tab.status, activeId === tab.id, isDark)}`} />
                  <span className="truncate">{shortTabTitle(tab.title)}</span>
                </button>
                <button
                  aria-label={t("vibe.closeTabAria", { title: tab.title })}
                  className={
                    isSkin
                      ? "vibe-skin-tab-close mr-1.5 grid h-5 w-5 shrink-0 place-items-center opacity-70 transition group-hover:opacity-100"
                      : isDark
                        ? "vibe-dark-tab-close mr-1.5 grid h-5 w-5 shrink-0 place-items-center opacity-70 transition group-hover:opacity-100"
                        : "vibe-light-tab-close mr-1.5 grid h-5 w-5 shrink-0 place-items-center opacity-70 transition group-hover:opacity-100"
                  }
                  onClick={() => void closeTab(tab)}
                  type="button"
                >
                  <X className="h-3 w-3" />
                </button>
              </div>
            ))}
          </div>

          <div
            className={
              isSkin
                ? "vibe-skin-terminal-shell m-2 min-h-0 flex-1 overflow-hidden border"
                : "min-h-0 flex-1 overflow-hidden"
            }
          >
            {!activeTab && (
              <div
                className={
                  isSkin
                    ? `vibe-scrollbar ${scrollbarThemeClass} vibe-skin-empty-state flex h-full min-h-0 flex-col justify-between gap-4 overflow-y-auto p-4 text-center`
                    : isDark
                    ? `vibe-scrollbar ${scrollbarThemeClass} flex h-full min-h-0 flex-col justify-between gap-4 overflow-y-auto p-4 text-center`
                    : `vibe-scrollbar ${scrollbarThemeClass} flex h-full min-h-0 flex-col justify-between gap-4 overflow-y-auto p-4 text-center`
                }
              >
                <div className="flex min-h-[9rem] flex-1 items-center justify-center pt-4">
                  <div>
                    <TerminalSquare
                      className={
                        isSkin
                          ? "mx-auto h-8 w-8 text-[var(--vibe-accent)]"
                          : isDark
                            ? "mx-auto h-8 w-8 text-[#586e75]"
                            : "mx-auto h-8 w-8 text-stone-400"
                      }
                    />
                    <p
                      className={
                        isSkin
                          ? "mt-2 text-sm font-semibold text-[var(--vibe-text)]"
                          : isDark
                            ? "mt-2 text-sm font-semibold text-[#fdf6e3]"
                            : "mt-2 text-sm font-semibold text-stone-900"
                      }
                    >
                      {launchTitle}
                    </p>
                    <p
                      className={
                        isSkin
                          ? "mt-1 text-[13px] text-[var(--vibe-muted-text)]"
                          : isDark
                            ? "mt-1 text-[13px] text-[#93a1a1]"
                            : "mt-1 text-[13px] text-stone-500"
                      }
                    >
                      {launchBody}
                    </p>
                  </div>
                </div>
                <section aria-label={launchTitle} className={launchPanelClass}>
                  <div className={agentStripClass}>
                    <div className="mb-2 flex flex-wrap items-center justify-between gap-2">
                      <div className="flex min-w-0 items-center gap-2">
                        {launchAgentPrefix && <span className={composerAddonClass}>{launchAgentPrefix}</span>}
                        <span className="truncate text-[12px] font-semibold">
                          {launchAgentStripLabel}
                        </span>
                      </div>
                      {launchAgentSuffix && <span className={composerAddonClass}>{launchAgentSuffix}</span>}
                    </div>
                    <div className="vibe-scrollbar vibe-scrollbar-horizontal flex gap-2 overflow-x-auto pb-1">
                      {agentOptions.map((option) => {
                        const active = createPlatform === option.platform;
                        return (
                          <button
                            aria-pressed={active}
                            className={agentOptionClass(active)}
                            key={option.platform}
                            onClick={() => setCreatePlatform(option.platform)}
                            type="button"
                          >
                            <AgentIcon className="h-5 w-5" platform={option.platform} />
                            <span>{option.label}</span>
                          </button>
                        );
                      })}
                    </div>
                  </div>

                  {(launchExtraLabel || launchExtraValue) && (
                    <div className="mt-3 flex flex-wrap gap-2">
                      {launchExtraLabel && <span className={composerAddonClass}>{launchExtraLabel}</span>}
                      {launchExtraValue && <span className={composerAddonClass}>{launchExtraValue}</span>}
                    </div>
                  )}

                  <div className={composerClass}>
                    <textarea
                      aria-label={launchPlaceholder}
                      className={composerInputClass}
                      onChange={(event) => setLaunchPrompt(event.target.value)}
                      placeholder={launchPlaceholder}
                      value={launchPrompt}
                    />
                    <div className={composerMetaBarClass}>
                      <label className="min-w-0 flex-1 text-[11px] font-semibold sm:max-w-[18rem]">
                        <span>{launchFolderLabel}</span>
                        <select
                          className={`${composerControlClass} truncate`}
                          onChange={handleLaunchFolderChange}
                          value={createProjectDir}
                        >
                          <option value="">{t("vibe.selectFolder")}</option>
                          {projectDirectories.map((directory) => (
                            <option key={directory} value={directory}>
                              {compactDirectoryLabel(directory)}
                            </option>
                          ))}
                          {customProjectDirectory && (
                            <option value={customProjectDirectory}>
                              {compactDirectoryLabel(customProjectDirectory)}
                            </option>
                          )}
                          <option value={chooseFolderOptionValue}>{t("vibe.launchNewFolder")}</option>
                        </select>
                      </label>
                      <label className="min-w-0 flex-1 text-[11px] font-semibold sm:max-w-[11rem]">
                        <span>{launchModelLabel}</span>
                        <select
                          className={composerControlClass}
                          onChange={(event) =>
                            setLaunchModel(
                              event.target.value as (typeof launchModelOptions)[number]["value"],
                            )
                          }
                          value={launchModel}
                        >
                          {launchModelOptions.map((option) => (
                            <option key={option.value} value={option.value}>
                              {option.label}
                            </option>
                          ))}
                        </select>
                      </label>
                      <label className="min-w-0 flex-1 text-[11px] font-semibold sm:max-w-[10rem]">
                        <span>{launchReasoningLabel}</span>
                        <select
                          className={composerControlClass}
                          onChange={(event) =>
                            setLaunchReasoning(
                              event.target
                                .value as (typeof launchReasoningOptions)[number]["value"],
                            )
                          }
                          value={launchReasoning}
                        >
                          {launchReasoningOptions.map((option) => (
                            <option key={option.value} value={option.value}>
                              {option.label}
                            </option>
                          ))}
                        </select>
                      </label>
                      <button
                        className={composerSendButtonClass}
                        onClick={launchFromComposer}
                        type="button"
                      >
                        <SendHorizontal className="h-4 w-4" />
                        <span>{launchSendLabel}</span>
                      </button>
                    </div>
                  </div>
                </section>
              </div>
            )}
            {tabs.map((tab) => (
              <XtermPane
                active={tab.id === activeId}
                key={tab.id}
                onStatusChange={updateStatus}
                session={tab}
                themeMode={terminalThemeMode}
                themeOverride={isSkin ? activeSkin.terminal : undefined}
                transparentSurface={isSkin}
              />
            ))}
          </div>
        </div>

        {showSkinRightRail && (
          <aside
            className={`vibe-scrollbar ${scrollbarThemeClass} vibe-scrollbar-active vibe-skin-right-rail hidden min-h-0 flex-col overflow-x-hidden overflow-y-auto border-l p-3 lg:flex`}
          >
            {skinBlocks.showcase.enabled && (
              <div className="vibe-skin-right-card flex shrink-0 flex-col rounded-3xl border p-3">
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0">
                    <p className="text-[10px] font-semibold tracking-[0.18em] text-[var(--vibe-muted-text)]">
                      {skinBlocks.showcase.badge}
                    </p>
                    {skinBlocks.showcase.title && (
                      <h2 className="mt-1 truncate text-lg font-semibold text-[var(--vibe-text)]">
                        {skinBlocks.showcase.title}
                      </h2>
                    )}
                    {skinBlocks.showcase.subtitle && (
                      <p className="mt-1 text-[12px] text-[var(--vibe-muted-text)]">
                        {skinBlocks.showcase.subtitle}
                      </p>
                    )}
                  </div>
                </div>
                <div className="vibe-skin-showcase-stage mt-3 flex min-h-[220px] flex-col items-center justify-center rounded-3xl border p-3 text-center">
                  {skinBlocks.showcase.figure ? (
                    <img
                      alt={`${skinBlocks.showcase.title || skinBlocks.showcase.badge || "skin showcase"} figure`}
                      className="vibe-skin-showcase-figure max-h-52 w-full max-w-[168px] object-contain"
                      src={skinBlocks.showcase.figure}
                    />
                  ) : (
                    renderSkinTemplateFigure(
                      decorations?.showcaseTemplate,
                      skinBlocks.showcase.title || skinBlocks.showcase.badge || "皮肤展示",
                    ) ?? <div className="vibe-skin-showcase-figure vibe-skin-showcase-orb grid h-32 w-28 place-items-center rounded-[2rem] border">
                      <AiSwitchLogo className="h-14 w-14 rounded-2xl" />
                    </div>
                  )}
                  {skinBlocks.showcase.body && (
                    <p className="mt-3 text-[13px] leading-6 text-[var(--vibe-text)] opacity-90">
                      {skinBlocks.showcase.body}
                    </p>
                  )}
                </div>
                <div className="vibe-skin-showcase-footer mt-3 rounded-2xl border px-3 py-2 text-[11px] text-[var(--vibe-muted-text)]">
                  {skinBlocks.showcase.footer}
                </div>
              </div>
            )}
            {skinRightCards.length ? (
              skinRightCards.map((card, index) => (
                <SkinDecorationCard
                  card={card}
                  key={`${card.template ?? "card"}-${card.title ?? index}`}
                  regionKeys={activeSkinRegionKeys}
                />
              ))
            ) : (
              <div className="vibe-skin-right-card mt-3 rounded-2xl border p-3">
                <p className="text-[10px] font-semibold tracking-[0.18em] text-[var(--vibe-muted-text)]">
                  皮肤区域
                </p>
                <div className="mt-2 flex flex-wrap gap-1.5">
                  {activeSkinRegionKeys.length > 0 ? (
                    activeSkinRegionKeys.slice(0, 8).map((region) => (
                      <span key={region} className="rounded-full border px-2 py-1 text-[11px]">
                        {region}
                      </span>
                    ))
                  ) : (
                    <span className="rounded-full border px-2 py-1 text-[11px]">ui</span>
                  )}
                </div>
              </div>
            )}
          </aside>
        )}
        </div>

        {taskbarEnabled ? (
          <div className="vibe-skin-taskbar relative flex h-10 shrink-0 items-center gap-2 border-t px-2 text-[11px] font-semibold">
            {startMenuOpen && (
              <div
                aria-label="开始菜单"
                className="vibe-skin-taskbar-start-menu absolute bottom-full left-2 z-40 mb-2 w-64 overflow-hidden rounded-2xl border p-2"
                ref={startMenuRef}
                role="menu"
              >
                <div className="mb-2 rounded-xl border border-white/50 bg-white/45 px-3 py-2 text-[12px] font-semibold text-[var(--vibe-text)]">
                  {skinBlocks.profile.name}
                </div>
                <div className="space-y-1">
                  {skinBlocks.taskbar.startMenu.items.map((item, index) =>
                    "type" in item ? (
                      <div
                        aria-orientation="horizontal"
                        className="my-1 h-px bg-[var(--vibe-border)]/70"
                        key={`separator-${index}`}
                        role="separator"
                      />
                    ) : (
                      <button
                        className="vibe-skin-taskbar-menu-item flex w-full items-center rounded-xl px-3 py-2 text-left text-[12px] transition disabled:cursor-not-allowed disabled:opacity-50"
                        disabled={item.disabled}
                        key={`${item.label}-${index}`}
                        onClick={() => runTaskbarMenuItem(item)}
                        role="menuitem"
                        type="button"
                      >
                        {item.label}
                      </button>
                    ),
                  )}
                </div>
              </div>
            )}

            <button
              aria-expanded={startMenuOpen}
              className="vibe-skin-taskbar-start-button inline-flex h-8 shrink-0 items-center gap-2 border px-4 text-[13px] font-bold italic tracking-wide transition"
              onClick={() => setStartMenuOpen((current) => !current)}
              ref={startButtonRef}
              type="button"
            >
              {skinBlocks.taskbar.startButton.icon && (
                <img
                  alt=""
                  aria-hidden="true"
                  className="h-4 w-4 shrink-0 object-contain"
                  src={skinBlocks.taskbar.startButton.icon}
                />
              )}
              <span>{skinBlocks.taskbar.startButton.label}</span>
            </button>

            <div className="flex min-w-0 flex-1 items-center gap-1.5">
              {skinBlocks.taskbar.items.map((item, index) => (
                <div
                  className={`${
                    item.active ? "vibe-skin-taskbar-item-active" : "vibe-skin-taskbar-item"
                  } flex h-7 min-w-0 max-w-[180px] items-center gap-2 rounded-lg border px-2 text-[11px]`}
                  key={`${item.label}-${index}`}
                >
                  {item.icon && (
                    <img
                      alt=""
                      aria-hidden="true"
                      className="h-4 w-4 shrink-0 object-contain"
                      src={item.icon}
                    />
                  )}
                  <span className="truncate">{item.label}</span>
                </div>
              ))}
              <span className="ml-1 hidden truncate text-[10px] font-medium opacity-90 sm:inline">
                {skinBlocks.statusbar.left}
              </span>
            </div>

            <div className="vibe-skin-taskbar-tray hidden h-7 shrink-0 items-center gap-1 rounded-lg border px-2 sm:flex">
              {skinBlocks.taskbar.tray.map((item, index) => (
                <span key={`${item}-${index}`} className="whitespace-nowrap">
                  {item}
                </span>
              ))}
              <span className="hidden whitespace-nowrap lg:inline">{skinBlocks.statusbar.right}</span>
            </div>
            <div className="vibe-skin-taskbar-clock flex h-7 shrink-0 items-center rounded-lg border px-2 tabular-nums">
              {currentTime}
            </div>
          </div>
        ) : isSkin ? (
          <div className="vibe-skin-status-bar flex h-9 shrink-0 items-center justify-between gap-3 border-t px-4 text-[11px] font-medium">
            <span className="truncate">{skinBlocks.statusbar.left}</span>
            <span className="truncate">{skinBlocks.statusbar.right}</span>
          </div>
        ) : null}
      </div>

      {appearanceOpen && (
        <div
          className="fixed inset-0 z-50 grid place-items-center bg-black/45 p-4"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget) {
              setAppearanceOpen(false);
            }
          }}
        >
          <div
            aria-labelledby="vibe-appearance-title"
            aria-modal="true"
            className={
              isSkin
                ? "vibe-skin-modal vibe-skin-panel-strong w-full max-w-md rounded-3xl border p-4 shadow-2xl"
                : isDark
                  ? "w-full max-w-md rounded-3xl border border-[#073642] bg-[#002b36] p-4 text-[#fdf6e3] shadow-2xl shadow-black/40"
                  : "w-full max-w-md rounded-3xl border border-stone-200 bg-white p-4 text-stone-950 shadow-2xl shadow-stone-950/15"
            }
            role="dialog"
          >
            <div className="mb-4 flex items-start justify-between gap-3">
              <div>
                <h2 id="vibe-appearance-title" className="text-base font-semibold">
                  {t("vibe.appearanceTitle")}
                </h2>
                <p className={isSkin ? "mt-1 text-[12px] text-[var(--vibe-muted-text)]" : isDark ? "mt-1 text-[12px] text-[#93a1a1]" : "mt-1 text-[12px] text-stone-500"}>
                  {t("vibe.appearanceSubtitle")}
                </p>
              </div>
              <button
                aria-label={t("vibe.cancel")}
                className={
                  isSkin
                    ? "vibe-skin-ghost grid h-8 w-8 place-items-center rounded-xl border transition"
                    : isDark
                      ? "grid h-8 w-8 place-items-center rounded-xl border border-[#586e75] text-[#93a1a1] transition hover:text-[#fdf6e3]"
                      : "grid h-8 w-8 place-items-center rounded-xl border border-stone-200 text-stone-500 transition hover:text-stone-950"
                }
                onClick={() => setAppearanceOpen(false)}
                type="button"
              >
                <X className="h-4 w-4" />
              </button>
            </div>

            <div className="space-y-4">
              <div>
                <p className={isSkin ? "mb-2 text-[12px] font-semibold text-[var(--vibe-muted-text)]" : isDark ? "mb-2 text-[12px] font-semibold text-[#93a1a1]" : "mb-2 text-[12px] font-semibold text-stone-600"}>
                  {t("vibe.themeChoices")}
                </p>
                <div className="grid grid-cols-3 gap-2">
                  {[
                    { mode: "dark" as const, label: t("vibe.themeDark") },
                    { mode: "light" as const, label: t("vibe.themeLight") },
                    { mode: "skin" as const, label: t("vibe.themeSkin") },
                  ].map((choice) => (
                    <button
                      aria-pressed={themeMode === choice.mode}
                      className={
                        themeMode === choice.mode
                          ? isSkin
                            ? "vibe-skin-primary rounded-xl border px-3 py-2 text-[12px] font-semibold"
                            : "rounded-xl bg-stone-950 px-3 py-2 text-[12px] font-semibold text-white"
                          : isSkin
                            ? "vibe-skin-ghost rounded-xl border px-3 py-2 text-[12px] font-semibold"
                            : isDark
                              ? "rounded-xl border border-[#586e75] px-3 py-2 text-[12px] font-semibold text-[#93a1a1]"
                              : "rounded-xl border border-stone-200 px-3 py-2 text-[12px] font-semibold text-stone-600"
                      }
                      key={choice.mode}
                      onClick={() => setThemeMode(choice.mode)}
                      type="button"
                    >
                      {choice.label}
                    </button>
                  ))}
                </div>
              </div>

              <label className={isSkin ? "block text-[12px] font-semibold text-[var(--vibe-muted-text)]" : isDark ? "block text-[12px] font-semibold text-[#93a1a1]" : "block text-[12px] font-semibold text-stone-600"}>
                {t("vibe.skinSelect")}
                <select
                  aria-label={t("vibe.skinSelect")}
                  className={
                    isSkin
                      ? "vibe-skin-select mt-1 w-full rounded-xl border px-3 py-2 text-[13px] outline-none transition"
                      : isDark
                        ? "mt-1 w-full rounded-xl border border-[#586e75] bg-[#073642] px-3 py-2 text-[13px] text-[#fdf6e3] outline-none focus:border-[#268bd2]"
                        : "mt-1 w-full rounded-xl border border-stone-200 bg-stone-50 px-3 py-2 text-[13px] text-stone-950 outline-none focus:border-blue-400"
                  }
                  onChange={(event) => {
                    setActiveSkinId(event.target.value);
                    setThemeMode("skin");
                  }}
                  value={activeSkinId}
                >
                  {availableSkins.map((skin) => (
                    <option key={skin.id} value={skin.id}>
                      {skin.name}
                    </option>
                  ))}
                </select>
              </label>

              <div className="flex flex-wrap gap-2">
                <button
                  className={
                    isSkin
                      ? "vibe-skin-ghost inline-flex items-center gap-2 rounded-xl border px-3 py-2 text-[12px] font-semibold transition"
                      : isDark
                        ? "inline-flex items-center gap-2 rounded-xl border border-[#586e75] px-3 py-2 text-[12px] font-semibold text-[#93a1a1] transition hover:text-[#fdf6e3]"
                        : "inline-flex items-center gap-2 rounded-xl border border-stone-200 px-3 py-2 text-[12px] font-semibold text-stone-600 transition hover:text-stone-950"
                  }
                  onClick={triggerSkinImport}
                  type="button"
                >
                  <Upload className="h-4 w-4" />
                  <span>{t("vibe.importSkinShort")}</span>
                </button>
                {customSkin && (
                  <button
                    className={
                      isSkin
                        ? "vibe-skin-danger rounded-xl border px-3 py-2 text-[12px] font-semibold transition"
                        : isDark
                          ? "rounded-xl border border-red-400/60 px-3 py-2 text-[12px] font-semibold text-red-200 transition hover:bg-red-500/20"
                          : "rounded-xl border border-red-200 px-3 py-2 text-[12px] font-semibold text-red-700 transition hover:bg-red-50"
                    }
                    onClick={clearCustomSkin}
                    type="button"
                  >
                    {t("vibe.clearSkin")}
                  </button>
                )}
              </div>
              <p className={isSkin ? "text-[12px] leading-5 text-[var(--vibe-muted-text)]" : isDark ? "text-[12px] leading-5 text-[#93a1a1]" : "text-[12px] leading-5 text-stone-500"}>
                {t("vibe.appearanceHelp")}
              </p>
            </div>
          </div>
        </div>
      )}

      {createDialogOpen && (
        <div className="fixed inset-0 z-50 grid place-items-center bg-black/55 p-4">
          <div
            className={
              isSkin
                ? "vibe-skin-modal vibe-skin-panel-strong w-full max-w-lg rounded-3xl border p-4 shadow-2xl"
                : isDark
                ? "w-full max-w-lg rounded-3xl border border-[#073642] bg-[#002b36] p-4 text-[#fdf6e3] shadow-2xl shadow-black/40"
                : "w-full max-w-lg rounded-3xl border border-stone-200 bg-white p-4 text-stone-950 shadow-2xl shadow-stone-950/15"
            }
            role="dialog"
            aria-modal="true"
            aria-labelledby="vibe-create-title"
          >
            <div className="mb-4 flex items-start justify-between gap-3">
              <div>
                <h2 id="vibe-create-title" className="text-base font-semibold">
                  {t("vibe.createTitle")}
                </h2>
                <p className={isSkin ? "mt-1 text-[12px] text-[var(--vibe-muted-text)]" : isDark ? "mt-1 text-[12px] text-[#93a1a1]" : "mt-1 text-[12px] text-stone-500"}>
                  {t("vibe.createSubtitle")}
                </p>
              </div>
              <button
                aria-label={t("vibe.cancel")}
                className={
                  isSkin
                    ? "vibe-skin-ghost grid h-8 w-8 place-items-center rounded-xl border transition"
                    : isDark
                    ? "grid h-8 w-8 place-items-center rounded-xl border border-[#586e75] text-[#93a1a1] transition hover:text-[#fdf6e3]"
                    : "grid h-8 w-8 place-items-center rounded-xl border border-stone-200 text-stone-500 transition hover:text-stone-950"
                }
                onClick={() => setCreateDialogOpen(false)}
                type="button"
              >
                <X className="h-4 w-4" />
              </button>
            </div>

            <div className="space-y-3">
              <label className={isSkin ? "block text-[12px] font-semibold text-[var(--vibe-muted-text)]" : isDark ? "block text-[12px] font-semibold text-[#93a1a1]" : "block text-[12px] font-semibold text-stone-600"}>
                {t("vibe.agent")}
                <select
                  className={
                    isSkin
                      ? "vibe-skin-field mt-1 w-full rounded-xl border px-3 py-2 text-[13px] outline-none transition"
                      : isDark
                      ? "mt-1 w-full rounded-xl border border-[#586e75] bg-[#073642] px-3 py-2 text-[13px] text-[#fdf6e3] outline-none focus:border-[#268bd2]"
                      : "mt-1 w-full rounded-xl border border-stone-200 bg-stone-50 px-3 py-2 text-[13px] text-stone-950 outline-none focus:border-blue-400"
                  }
                  onChange={(event) =>
                    setCreatePlatform(event.target.value as (typeof agentOptions)[number]["platform"])
                  }
                  value={createPlatform}
                >
                  {agentOptions.map((option) => (
                    <option key={option.platform} value={option.platform}>
                      {option.label}
                    </option>
                  ))}
                </select>
              </label>

              {projectDirectories.length > 0 && (
                <label className={isSkin ? "block text-[12px] font-semibold text-[var(--vibe-muted-text)]" : isDark ? "block text-[12px] font-semibold text-[#93a1a1]" : "block text-[12px] font-semibold text-stone-600"}>
                  {t("vibe.existingFolder")}
                  <select
                    className={
                      isSkin
                        ? "vibe-skin-field mt-1 w-full rounded-xl border px-3 py-2 text-[13px] outline-none transition"
                        : isDark
                        ? "mt-1 w-full rounded-xl border border-[#586e75] bg-[#073642] px-3 py-2 text-[13px] text-[#fdf6e3] outline-none focus:border-[#268bd2]"
                        : "mt-1 w-full rounded-xl border border-stone-200 bg-stone-50 px-3 py-2 text-[13px] text-stone-950 outline-none focus:border-blue-400"
                    }
                    onChange={(event) => setCreateProjectDir(event.target.value)}
                    value={createProjectDir}
                  >
                    <option value="">{t("vibe.selectFolder")}</option>
                    {projectDirectories.map((directory) => (
                      <option key={directory} value={directory}>
                        {compactDirectoryLabel(directory)}
                      </option>
                    ))}
                  </select>
                </label>
              )}

              <label className={isSkin ? "block text-[12px] font-semibold text-[var(--vibe-muted-text)]" : isDark ? "block text-[12px] font-semibold text-[#93a1a1]" : "block text-[12px] font-semibold text-stone-600"}>
                {t("vibe.projectDirectory")}
                <div className="mt-1 flex gap-2">
                  <input
                    className={
                      isSkin
                        ? "vibe-skin-field min-w-0 flex-1 rounded-xl border px-3 py-2 text-[13px] outline-none transition"
                        : isDark
                        ? "min-w-0 flex-1 rounded-xl border border-[#586e75] bg-[#073642] px-3 py-2 text-[13px] text-[#fdf6e3] outline-none placeholder:text-[#586e75] focus:border-[#268bd2]"
                        : "min-w-0 flex-1 rounded-xl border border-stone-200 bg-stone-50 px-3 py-2 text-[13px] text-stone-950 outline-none focus:border-blue-400"
                    }
                    onChange={(event) => setCreateProjectDir(event.target.value)}
                    placeholder={t("vibe.projectPlaceholder")}
                    value={createProjectDir}
                  />
                  <button
                    className={
                      isSkin
                        ? "vibe-skin-ghost shrink-0 rounded-xl border px-3 py-2 text-[13px] font-semibold transition"
                        : isDark
                        ? "shrink-0 rounded-xl border border-[#586e75] bg-[#073642] px-3 py-2 text-[13px] font-semibold text-[#fdf6e3] transition hover:border-[#839496]"
                        : "shrink-0 rounded-xl border border-stone-200 bg-white px-3 py-2 text-[13px] font-semibold text-stone-700 transition hover:border-stone-300"
                    }
                    onClick={() => void chooseFolder()}
                    type="button"
                  >
                    {t("vibe.chooseFolder")}
                  </button>
                </div>
              </label>
            </div>

            <div className="mt-5 flex justify-end gap-2">
              <button
                className={
                  isSkin
                    ? "vibe-skin-ghost rounded-xl border px-3 py-2 text-[13px] font-semibold transition"
                    : isDark
                    ? "rounded-xl border border-[#586e75] px-3 py-2 text-[13px] font-semibold text-[#93a1a1] transition hover:text-[#fdf6e3]"
                    : "rounded-xl border border-stone-200 px-3 py-2 text-[13px] font-semibold text-stone-600 transition hover:text-stone-950"
                }
                onClick={() => setCreateDialogOpen(false)}
                type="button"
              >
                {t("vibe.cancel")}
              </button>
              <button
                className={
                  isSkin
                    ? "vibe-skin-primary rounded-xl border px-3 py-2 text-[13px] font-semibold transition"
                    : isDark
                    ? "rounded-xl bg-[#b58900] px-3 py-2 text-[13px] font-semibold text-[#002b36] transition hover:bg-[#cb4b16] hover:text-white"
                    : "rounded-xl bg-stone-950 px-3 py-2 text-[13px] font-semibold text-white transition hover:bg-stone-800"
                }
                onClick={launchNewAgent}
                type="button"
              >
                {t("vibe.create")}
              </button>
            </div>
          </div>
        </div>
      )}
    </main>
  );
}
