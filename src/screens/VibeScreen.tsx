import { useQuery } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { MotionMenu } from "../components/motion/MotionPrimitives";
import { open } from "@tauri-apps/plugin-dialog";
import {
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Columns3,
  FolderOpen,
  MoonStar,
  Palette,
  PanelLeftClose,
  Play,
  Plus,
  SendHorizontal,
  Settings2,
  SunMedium,
  TerminalSquare,
  Upload,
  X,
} from "lucide-react";
import { useCallback, useEffect, useId, useMemo, useRef, useState } from "react";
import type {
  ChangeEvent,
  CSSProperties,
  MouseEvent as ReactMouseEvent,
  ReactNode,
} from "react";
import {
  createTerminalSession,
  killTerminalSession,
  listAgentLaunchOptions,
  listSessions,
} from "../lib/api/client";
import { useI18n } from "../lib/i18n";
import { isDesktop } from "../lib/transport";
import { useDragResize } from "../lib/useDragResize";
import {
  BUILT_IN_VIBE_SKINS,
  clearStoredVibeSkin,
  getVibeSkinBlocks,
  importVibeSkinPackage,
  VIBE_SKIN_REGION_KEYS,
  readStoredVibeAppearance,
  readStoredVibeSkin,
  skinToCssVariables,
  writeStoredVibeAppearance,
  writeStoredVibeSkin,
} from "../lib/vibeSkin";
import type {
  VibeAppearanceTheme,
  VibeSkinAudioEvent,
  VibeSkinDecorationCard,
  VibeSkinDecorationItem,
  VibeSkinDecorationTemplate,
  VibeSkinDecorationTone,
  VibeSkinDecorationVariant,
  VibeSkinDefinition,
  VibeSkinTaskbarMenuItem,
} from "../lib/vibeSkin";
import {
  readStoredVibeTabs,
  writeStoredVibeTabs,
  type VibeTabDescriptor,
} from "../lib/vibeTabs";
import { AiSwitchLogo } from "../components/brand/AiSwitchLogo";
import type {
  AgentLaunchOption,
  CreateTerminalSessionInput,
  SessionMeta,
  TerminalSession,
  TerminalStatus,
} from "../lib/api/types";
import { XtermPane } from "../components/terminal/XtermPane";
import { StarshipHologram } from "../components/vibe/StarshipHologram";

const isJSDOM = typeof navigator !== "undefined" && /jsdom/i.test(navigator.userAgent);

const agentOptions = [
  { platform: "codex", label: "Codex" },
  { platform: "claude", label: "Claude" },
  { platform: "grok", label: "Grok" },
  { platform: "gemini", label: "Gemini" },
  { platform: "opencode", label: "OpenCode" },
  { platform: "openclaw", label: "OpenClaw" },
  { platform: "hermes", label: "Hermes" },
] as const;

const chooseFolderOptionValue = "__choose_folder__";
const autoLaunchOptionValue = "auto";

// The session list is a grid track, so its width lives in a CSS variable that both
// the plain and skinned layouts read.
const SESSION_LIST_DEFAULT_WIDTH = 356;
const SESSION_LIST_SKIN_DEFAULT_WIDTH = 300;
const SESSION_LIST_MIN_WIDTH = 220;
const SESSION_LIST_MAX_WIDTH = 560;

function clampSessionListWidth(value: number) {
  return Math.min(Math.max(Math.round(value), SESSION_LIST_MIN_WIDTH), SESSION_LIST_MAX_WIDTH);
}

// Below this width the session list stops being a grid track: it hides and only
// reappears as a fixed drawer, so the workspace keeps the full window height.
const SESSION_LIST_DRAWER_BREAKPOINT = 1024;

// Tiled terminals never shrink below a usable width, so the tile size is a user
// preference exposed through a CSS variable the stylesheet reads.
const TILE_DEFAULT_WIDTH = 448;
const TILE_MIN_WIDTH = 320;
const TILE_MAX_WIDTH = 960;
const TILE_WIDTH_STEP = 16;

function clampTileWidth(value: number) {
  return Math.min(Math.max(Math.round(value), TILE_MIN_WIDTH), TILE_MAX_WIDTH);
}

type AgentPlatform = (typeof agentOptions)[number]["platform"];

type VibeTheme = VibeAppearanceTheme;

type VibeScreenProps = {
  onExitVibe?: () => void;
};

type AmbientAudioHandle = {
  audio: HTMLAudioElement;
  intervalId?: number;
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

// Sub-agent runs and slash-command bookkeeping surface as sessions whose title is
// a raw <local-command-*> or <command-*> block. They are never worth resuming
// from the list.
const BOOKKEEPING_TITLE_PREFIXES = ["<command-name", "<command-message", "<command-args"];

function isBookkeepingSession(session: SessionMeta) {
  const title = (session.title ?? "").trim().toLowerCase();
  if (title.includes("<local-command-")) {
    return true;
  }
  return BOOKKEEPING_TITLE_PREFIXES.some((prefix) => title.startsWith(prefix));
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

function GrokAgentSvg({ size = "100%" }: AgentSvgProps) {
  return (
    <svg
      fill="currentColor"
      height={size}
      style={baseAgentSvgStyle}
      viewBox="48 48 416 416"
      width={size}
      xmlns="http://www.w3.org/2000/svg"
    >
      <path d="M210.484 312.759L343.465 210.383C349.984 205.364 359.302 207.322 362.408 215.117C378.758 256.231 371.454 305.64 338.925 339.563C306.397 373.487 261.137 380.927 219.768 363.983L174.577 385.803C239.394 432.008 318.104 420.581 367.289 369.251C406.303 328.564 418.386 273.104 407.088 223.091L407.19 223.198C390.807 149.726 411.218 120.359 453.03 60.3072C454.02 58.8833 455.01 57.4595 456 56L400.978 113.382V113.204L210.45 312.794" />
      <path d="M183.042 337.641C136.519 291.294 144.54 219.567 184.236 178.203C213.59 147.59 261.683 135.096 303.666 153.464L348.755 131.75C340.632 125.627 330.221 119.042 318.275 114.414C264.277 91.2407 199.63 102.774 155.735 148.516C113.513 192.549 100.236 260.254 123.036 318.027C140.069 361.206 112.148 391.748 84.0229 422.575C74.0561 433.503 64.0553 444.431 56 456L183.007 337.677" />
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
      fill="currentColor"
      fillRule="evenodd"
      height={size}
      style={baseAgentSvgStyle}
      viewBox="0 0 24 24"
      width={size}
      xmlns="http://www.w3.org/2000/svg"
    >
      <path d="M5.938 12.835c.127-.039.285.02.373.143.028.038.036.092.046.14.003.014-.02.033-.04.05-.124-.098-.24-.194-.354-.291-.011-.01-.016-.027-.025-.042zM8.396 9.412c.195-.032.39-.06.588-.05a.54.54 0 01.148.026c.202.071.402.147.601.224.028.01.05.036.075.055l-.013.027a9.203 9.203 0 01-.26-.089c-.115-.038-.213-.077-.315-.098-.25-.05-.25-.046-.292-.014l.574.144c.275.139.55.276.823.417.042.022.09.057.107.098.026.06.063.076.117.072.066-.006.132-.017.213-.027l-.04.086c.051.08.142.02.216.064-.074.13-.247.09-.334.199l.061.074-.12.087c0 .106-.038.168-.306.243l.026.085-.196.042.07.124h-.25l-.007.137c-.081-.01-.161-.018-.244-.027l-.053.123c-.027-.008-.052-.011-.073-.023-.067-.038-.128-.056-.195.006-.019.017-.063.014-.093.008-.026-.006-.05-.029-.07-.042-.11.095-.11.095-.208.003-.057.046-.12.074-.186.011-.063.027-.123-.02-.178-.014-.07.007-.097-.035-.133-.07l-.13.033c-.013-.236-.194-.19-.34-.203.005-.072.05-.092.095-.094a.474.474 0 01.159.022c.164.05.32.12.496.138.203.021.405.029.601-.015.265-.059.52-.149.707-.365.049-.056.083-.127.117-.195.019-.038.02-.084-.02-.116a1.397 1.397 0 00-.382-.217c.024.12-.031.182-.115.221 0 .014-.004.025 0 .03.08.115.084.16-.007.267a1.39 1.39 0 01-.218.211.477.477 0 01-.641-.05 1.36 1.36 0 01-.133-.152c-.078-.107-.076-.108-.033-.236-.165-.08-.128-.226-.104-.364.008-.05.028-.096.049-.163-.04.014-.067.017-.087.032a.897.897 0 00-.316.357c-.007.016-.01.034-.02.047-.012.015-.034.038-.045.035-.02-.006-.037-.027-.05-.045-.008-.012-.007-.032-.012-.057h-.126l.053-.172a14.82 14.82 0 00-.039-.049l.11-.284c-.06.026-.091.044-.124.051-.03.007-.064 0-.095 0 0-.031-.01-.07.004-.092.149-.22.305-.428.593-.476z" />
      <path d="M8.06 10.788c-.003-.038-.004-.075.037-.062.016.006.034.048.028.067-.01.04-.038.032-.064-.005z" />
      <path
        clipRule="evenodd"
        d="M11.981.009c.226-.012.453-.011.679 0 .247.01.495.024.74.062.401.064.798.157 1.19.273.463.138.92.299 1.356.511a7.31 7.31 0 012.948 2.642c.292.469.536.963.739 1.479.219.556.446 1.11.623 1.683.204.654.329 1.326.458 1.997.097.504.182 1.01.29 1.511.156.722.329 1.44.494 2.16.186.812.4 1.615.63 2.415.102.355.193.713.282 1.072.11.436.202.876.254 1.323.031.278.066.557.073.837a7.56 7.56 0 01-.017.88c-.037.413-.1.818-.226 1.212a5.017 5.017 0 01-.915 1.649l-.13.156.018.023c.043-.023.088-.041.127-.068.2-.138.373-.307.531-.49.4-.46.721-.973.975-1.529a3.59 3.59 0 00.325-1.72c-.024-.424-.097-.834-.3-1.213-.013-.027-.015-.06-.03-.121.05.035.082.048.101.072.107.13.22.258.315.398.33.494.46 1.052.486 1.64a3.75 3.75 0 01-.47 1.97c-.36.655-.887 1.14-1.526 1.506-.193.111-.394.21-.595.308-.157.078-.248.211-.318.365a.522.522 0 00-.033.406.359.359 0 01.013.139c-.005.077-.077.155-.14.162-.054.006-.125-.043-.15-.116a1.206 1.206 0 01-.06-.233c-.04-.314-.155-.6-.308-.87a3.906 3.906 0 00-.73-.91 2.129 2.129 0 00-.897-.524 4.093 4.093 0 00-.692-.131c-.075-.008-.15-.04-.22.01.18.06.363.11.538.18.434.173.82.43 1.18.728.308.255.58.543.794.884.098.155.186.315.227.496.027.123.042.25.067.375.013.062-.002.109-.053.144-.047.033-.122.034-.163-.01a.455.455 0 01-.08-.14c-.03-.073-.038-.159-.078-.225a7.314 7.314 0 00-1.423-1.664c-.16-.137-.329-.26-.537-.323-.376-.114-.753-.203-1.15-.154-.213.025-.427.032-.64.053a1.6 1.6 0 00-.736.278 5.14 5.14 0 00-.834.72c-.329.342-.642.699-.955 1.055-.136.155-.264.319-.314.531a5.227 5.227 0 00-.012.051.096.096 0 01-.09.076h-.31c-.046 0-.082-.048-.072-.094.023-.108.045-.216.07-.324.075-.325.19-.635.368-.917.024-.039.04-.088.104-.08l.01.049.027.077c.28-.435.571-.834.996-1.135.283-.204.584-.378.89-.55a.196.196 0 00-.098-.002c-.162.043-.325.084-.485.134-.402.124-.764.33-1.11.566-.147.1-.298.193-.414.333a7.314 7.314 0 00-1.07 1.767.845.845 0 00-.04.12.075.075 0 01-.072.056h-.494c-.04 0-.062-.051-.036-.082.123-.14.246-.282.377-.415.275-.281.58-.532.777-.884.027-.048.063-.09.095-.135.238-.333.54-.607.818-.902.082-.086.175-.16.26-.24.029-.027.053-.057.079-.085l-.018-.025-.135.041c-.034.017-.07.031-.102.05-.248.144-.494.292-.743.433-.408.23-.825.439-1.209.711-.281.2-.591.358-.889.533-.02.012-.044.015-.08.028-.015-.135.143-.201.108-.336-.033.014-.064.02-.085.038-.111.096-.227.19-.328.296-.148.157-.284.325-.425.488-.125.143-.25.286-.373.431A.153.153 0 019.89 24H8.762a.316.316 0 00.016-.042c.028-.09.085-.172.083-.28-.091-.018-.162.001-.212.077a4.45 4.45 0 00-.136.215c-.01.016-.024.03-.042.03h-.093c-.019 0-.029-.022-.017-.037.071-.088.14-.178.209-.268.001-.002-.006-.012-.012-.024-.014.004-.03.006-.045.013-.176.09-.352.181-.527.274a.363.363 0 01-.168.042H5.202c-.026 0-.039-.036-.019-.053.21-.178.402-.374.558-.605.335-.496.538-1.047.667-1.629.004-.02-.003-.043-.006-.091-.037.048-.059.072-.076.1a1.943 1.943 0 01-.334.415c-.28.258-.59.448-.983.464-.297.012-.588 0-.865-.127-.46-.21-.722-.57-.794-1.072-.025-.17-.017-.171-.182-.219A3.513 3.513 0 011.97 20.6a2.286 2.286 0 01-.808-1.13 3.569 3.569 0 01-.16-1.245c.002-.034.016-.067.024-.1.032.023.046.043.05.066.033.153.059.308.096.46.086.355.257.664.516.92.258.256.571.419.91.532.358.118.717.138 1.07-.016a1.89 1.89 0 00.621-.452c.328-.348.533-.76.648-1.223.009-.034.005-.071.007-.11-.015.006-.026.006-.03.011-.031.05-.064.1-.093.152-.284.502-.679.887-1.196 1.135-.351.17-.718.255-1.11.159a1.607 1.607 0 01-.971-.64 2.006 2.006 0 01-.368-.924 2.903 2.903 0 01.02-.886c.05-.439.466-1.17.742-1.271-.02.063-.035.112-.053.16-.043.116-.097.227-.13.345a1.901 1.901 0 00-.05.82c.033.212.09.416.204.6.147.236.346.407.62.465.11.023.225.014.338.018a.576.576 0 00.386-.131c.164-.128.282-.292.366-.481.168-.375.24-.777.309-1.179.05-.296.093-.594.133-.893.039-.281.071-.563.104-.845.026-.232.048-.464.074-.696.024-.228.052-.455.076-.683.024-.227.047-.455.069-.683.013-.14.022-.28.034-.42l.037-.417c.022-.25.041-.5.065-.748.008-.082-.02-.132-.09-.177a2.46 2.46 0 01-.492-.418c-.1-.109-.188-.228-.282-.342-.035-.042-.056-.097-.116-.118a2.084 2.084 0 00.275.597c.06.092.131.176.196.265.063.086.182.115.234.226-.028.003-.046.01-.06.006a4.74 4.74 0 01-.22-.057 2.71 2.71 0 01-1.287-.819c-.435-.487-.656-1.076-.71-1.723a5.206 5.206 0 01.014-1.06c.072-.602.22-1.186.45-1.745.155-.376.338-.741.526-1.102.205-.393.466-.75.765-1.076.512-.559 1.104-1.024 1.726-1.448.717-.49 1.478-.898 2.277-1.233C8.244.828 8.767.632 9.31.494c.655-.166 1.31-.33 1.982-.415.229-.03.458-.058.688-.07zm-1.847 22.82c-.07.06-.147.111-.207.18-.238.27-.464.549-.668.869l-.044.108a.177.177 0 00.093-.057c.174-.19.351-.378.519-.574.104-.122.195-.255.288-.386.024-.034.03-.08.046-.12l-.027-.02zm1.65-3.695a5.51 5.51 0 00-.653.593l-.37.386a.963.963 0 01-.377.25 1.372 1.372 0 01-.467.09c-.044 0-.087.006-.151.012.028.058.043.097.064.131.15.242.301.482.45.724.136.22.276.438.399.666.068.125.105.267.156.404.077.027.14-.018.202-.048.29-.135.579-.274.867-.412.213-.101.437-.186.636-.31.347-.215.68-.455 1.018-.685.015-.01.026-.028.042-.046-.023-.019-.038-.037-.056-.044-.287-.111-.527-.3-.77-.482a5.319 5.319 0 01-.506-.42 1.757 1.757 0 01-.41-.653c-.019-.049-.045-.095-.075-.156zm-5.847.264c-.06.096-.097.194-.132.293a3.38 3.38 0 01-.555 1.01c-.2.25-.455.412-.762.493-.23.06-.464.076-.7.07-.048-.002-.097.002-.158.005.016.04.021.066.035.085.1.145.23.246.4.295.157.046.316.034.498.023.181-.037.343-.115.485-.234.238-.199.402-.454.536-.732.175-.363.264-.751.342-1.144.01-.053.008-.11.011-.164zm14.945-4.586c.008.029.016.057.027.107.024.155.051.31.072.464.03.219.067.437.078.657.017.344.027.689-.014 1.033-.037.315-.063.633-.116.946a6.153 6.153 0 01-.46 1.518c-.008.018-.01.039-.02.082.047-.03.077-.042.098-.064.085-.083.17-.167.248-.255.271-.305.458-.66.596-1.043.18-.498.228-1.011.145-1.531-.103-.65-.33-1.263-.597-1.881a9.055 9.055 0 00-.024-.055l-.033.022zM5.797 8.29a.26.26 0 00.018.153c.124.251.25.501.379.75.025.049.066.09.03.163-.284.06-.578.119-.88.255.059.038.097.06.132.087.042.032.112.058.09.12-.01.033-.075.048-.117.072.017.01.043.021.067.036.166.102.33.207.447.368.138.192.229.404.188.644-.079.469-.306.85-.69 1.132-.054.04-.106.083-.161.122a.243.243 0 00-.103.245.77.77 0 00.055.195c.083.196.22.35.375.492.083.076.159.164.222.257a.37.37 0 01.025.377c-.023.05-.05.099-.076.148-.03.06-.028.111.022.162.041.042.08.089.112.138.038.058.078.079.147.05a.486.486 0 01.333-.006c.16.046.302.126.444.21.13.077.264.149.4.219.067.035.14.05.219.026.071-.022.124.01.145.076.02.064-.003.108-.074.139-.07.03-.137.063-.209.088-.1.035-.201.073-.314.077-.013-.107.11-.088.127-.159-.206-.126-.643-.145-.801-.034.063.112.035.21-.096.313-.13-.1-.025-.202.002-.3a.209.209 0 00-.249.17c-.015.101.067.216.178.224.108.007.218-.005.326-.012.06-.005.12-.027.199 0-.103.123-.248.127-.357.19.002.05.07.086.019.131-.053.048-.095-.001-.132-.03-.08-.063-.16-.126-.231-.197a.474.474 0 01-.157-.311.52.52 0 00-.043-.172c-.032-.074-.032-.137.033-.19-.018-.03-.028-.053-.045-.072a1.222 1.222 0 01-.196-.369c-.053-.137-.046-.264.048-.381.024-.03.05-.06.064-.095a.664.664 0 00.047-.168c.017-.165-.064-.287-.182-.387-.186-.156-.36-.322-.46-.551-.005-.011-.024-.017-.037-.026-.011.017-.024.027-.025.038-.019.185-.045.37-.052.557-.014.377.058.743.162 1.104.118.41.289.798.488 1.173.267.502.537 1.002.812 1.5.055.098.13.189.208.27.198.202.452.272.724.273.202 0 .404-.006.605-.026.295-.03.59-.073.884-.113.183-.025.365-.057.548-.08.21-.026.38.073.522.21.16.156.305.327.447.5.22.265.397.56.554.867.05.098.07.1.147.03.13-.121.26-.242.394-.36.067-.059.088-.12.067-.213a3.535 3.535 0 01-.085-.796c.002-.157.006-.314.018-.471.015-.224.03-.45.06-.672a59.114 59.114 0 01.362-2.298c.087-.493.182-.984.268-1.477.06-.347.118-.694.162-1.043.034-.273.055-.55.063-.825.011-.332.003-.665.002-.998 0-.077.004-.155-.01-.23-.028-.142-.01-.155-.162-.19a5.826 5.826 0 00-.607-.107c-.146-.018-.207-.053-.221-.19-.006-.049-.025-.098-.041-.146-.009-.025-.024-.048-.046-.09l-.025.264c-.009.096-.029.116-.127.115-.055 0-.11-.008-.164-.008-.476 0-.952-.008-1.426.032-.095.008-.173-.015-.226-.103-.04-.066-.088-.126-.134-.186-.063-.084-.086-.093-.182-.06-.195.068-.388.138-.582.21a2.71 2.71 0 00-.675.394.986.986 0 01-.323.168c-.033.01-.07.008-.127.013.02-.066.024-.114.047-.15.064-.105.135-.205.205-.306.023-.033.049-.063.073-.095l-.015-.023-.201.037c-.146.04-.296.07-.437.122-.148.053-.266.023-.386-.072a3.623 3.623 0 01-.733-.786l-.093-.132zm8.592 8.963l-.147.09c-.22.134-.44.266-.659.402-.093.058-.184.12-.27.188-.085.07-.124.161-.072.272.047.1.093.2.147.294.047.08.124.138.213.147.11.01.228.012.336-.012.217-.05.372-.205.528-.357a.291.291 0 00.087-.308c-.046-.18-.079-.365-.118-.547-.011-.052-.027-.103-.045-.169zm-.257-2.409c-.12.291-.205.597-.325.91-.151.433-.294.87-.435 1.323.036-.01.054-.01.067-.018.261-.16.522-.324.785-.484.054-.033.071-.078.065-.138-.012-.13-.024-.262-.034-.393l-.068-.886c-.008-.103-.02-.206-.029-.31-.009 0-.017-.002-.026-.004zm3.081-8.13l.099.285c.08.231.159.463.24.714l.58 1.952c.187.63.372 1.262.558 1.893.114.382.235.762.343 1.146.072.257.126.519.186.799.044.206.087.413.127.64.034.106.023.226.077.325l.025-.006-.068-.362c-.038-.206-.077-.412-.113-.638-.015-.07-.029-.141-.046-.211-.095-.396-.177-.796-.29-1.187-.196-.685-.413-1.364-.618-2.046-.165-.549-.322-1.1-.488-1.648-.069-.227-.15-.45-.226-.695l-.117-.336c-.037-.107-.075-.216-.115-.322-.04-.106-.084-.21-.127-.314a7.558 7.558 0 01-.027.01zM6.225 14.304c-.063-.001-.115.014-.134.083a.35.35 0 00.41.012 4.533 4.533 0 00-.276-.095zM5.23 11.98c-.026-.027-.057-.048-.075.002-.012.032-.007.07-.01.113.082-.037.082-.037.085-.115zm.062-1.189a.135.135 0 00-.088.056.197.197 0 00-.025.11c.005.152.01.306.026.457a.751.751 0 00.066.218c.061.136.157.167.288.101.055-.027.06-.054.025-.11a4.52 4.52 0 01-.129-.211c-.015-.068-.066-.131-.033-.207.04-.09-.076-.116-.074-.19V10.874c-.003-.038-.006-.087-.056-.083zm-.017-.968a.867.867 0 00-.467.127c-.076.045-.084.07-.05.158.034.087.07.173.115.254.064.117.09.125.21.077a.657.657 0 01.336-.053c.202.022.357.136.504.264l.092.077c.007-.006.014-.013.022-.018-.019-.105-.035-.226-.149-.264-.157-.053-.324-.075-.508-.117l-.24-.005c.24-.169.452-.044.687.009-.063-.115-.153-.147-.23-.193-.082-.05-.17-.092-.25-.144-.06-.037-.12-.08-.072-.172zm10.233.325c-.23-.01-.427.08-.608.211-.034.026-.06.065-.105.117.087.026.15.046.232.065.044-.015.088-.03.13-.046.306-.114.61-.115.904.031.126.063.237.04.366-.005-.02-.031-.03-.054-.045-.071a.986.986 0 00-.448-.273c-.14-.044-.284-.024-.426-.03zM7.99 6.483a.308.308 0 00.002.133c.08.321.156.643.242.962.104.387.27.75.456 1.103.02.037.061.08.098.087a.404.404 0 00.253-.051l-.472-.84c-.23-.448-.405-.92-.579-1.394zM10.397.497c-.2-.008-.405.004-.603.034-.236.035-.47.087-.7.152-.287.08-.569.18-.852.273-.04.013-.074.038-.11.058.028.014.05.018.07.014.287-.068.58-.085.873-.09.134-.002.269.009.402.025.19.024.382.048.57.09.456.104.874.3 1.265.556.464.306.888.66 1.257 1.078.205.232.395.475.56.739.17.274.315.561.449.856.273.601.456 1.232.6 1.876.04.173.07.348.1.524.017.104.065.167.17.19.122.028.2.105.22.251-.003.102-.06.174-.129.24a1.065 1.065 0 00-.268.358.164.164 0 00.083-.039c.08-.086.162-.172.235-.265a.56.56 0 00.13-.333c.009-.05.022-.1.024-.15.007-.124-.017-.15-.143-.168-.025-.004-.049-.014-.073-.015-.082-.007-.125-.063-.137-.131-.033-.198-.004-.355.247-.408.086-.018.174-.03.26-.042.158-.023.315-.053.473-.067.14-.012.19.033.226.167.008.029.018.057.021.087.019.179-.008.225-.141.288-.027.013-.055.024-.078.042a.148.148 0 00-.051.067c-.039.144.073.382.206.445l.673.32c.023.011.05.015.075.023l.018-.026c-.015-.008-.032-.013-.044-.024a2.27 2.27 0 00-.544-.32 4.898 4.898 0 00-.173-.075.203.203 0 01-.126-.191c-.003-.085.045-.154.128-.187l.059-.025c.099-.044.118-.076.112-.187a.384.384 0 00-.008-.063c-.067-.294-.123-.59-.205-.88a9.478 9.478 0 00-.826-2.036 7.465 7.465 0 00-1.39-1.805 4.536 4.536 0 00-1.177-.824 3.656 3.656 0 00-1.016-.328 6.155 6.155 0 00-.712-.074zm6.719 5.955c.01.014.018.028.038.034l-.022-.044-.016.01zM4.103 3.917a.062.062 0 01-.03.012.455.455 0 01-.04.039c-.01.01-.02.02-.045.04l-.363.354c-.088.085-.17.178-.266.253-.284.22-.425.53-.544.855a.132.132 0 00-.007.071c.013.055.033.108.052.168l.074.026c-.017.056-.03.105-.047.152-.058.164-.118.327-.175.491-.005.015.008.036.019.077.08-.175.158-.33.225-.489.228-.544.484-1.074.819-1.561.09-.133.182-.266.283-.401.004-.006.007-.013.022-.03.001-.016.003-.032.015-.04l.008-.017zm12.976 2.408a.023.023 0 01.009.019.073.073 0 00-.006.01.188.188 0 00.007.02l.018.022c.002-.007.007-.016.005-.021-.003-.01-.012-.018-.02-.038a1.331 1.331 0 01-.013-.012zM4.199 4.48c-.003.004-.008.008-.027.014-.005.013-.011.025-.031.047a2.085 2.085 0 01-.124.167c-.048.07-.116.055-.181.041-.134-.028-.228.016-.287.143-.089.187-.187.37-.273.56-.049.108-.11.216-.118.36.081.003.154.007.228.008h.228a2.563 2.563 0 01-.079.264c-.01.052-.022.103-.033.155l.02.004c.018-.046.037-.092.067-.153.066-.142.13-.285.2-.426.02-.04.034-.1.116-.092 0 .043.004.084 0 .124-.005.045-.017.09-.028.143.141.043.086.174.115.269.102-.022.104-.195.248-.144v.205l.017.002.439-1.059c-.13 0-.246-.02-.358.033-.024.011-.058-.001-.108-.004.075-.15.139-.278.211-.417a.128.128 0 01.025-.036c0-.015-.001-.03.008-.038l.006-.02c-.005.006-.01.011-.028.017-.004.012-.009.024-.026.045a.085.085 0 01-.032.033c-.123.157-.09.164-.258.106-.079-.027-.078-.028-.047-.144.028-.046.056-.093.098-.15 0-.016-.001-.032.007-.042L4.2 4.48zm2.073-.67c-.003.006-.007.011-.027.016-.094.125-.194.246-.28.377-.155.238-.301.481-.451.723-.14.224-.345.368-.575.481-.017.008-.04.006-.079.011.012-.059.016-.109.033-.153a6.076 6.076 0 01.229-.518l-.007-.02a.138.138 0 01-.035.025c-.028.05-.055.1-.093.164-.26.424-.443.817-.442.95.024.004.048.011.073.013.177.013.188.007.26-.165.03-.07.077-.12.147-.15l.175-.07c.044-.018.085-.057.146-.032.003.05-.01.11.014.145.042.062.044.125.047.193.002.049.017.098.026.147.029-.034.039-.065.05-.097.142-.39.277-.782.428-1.17.1-.256.22-.504.33-.756.013-.03.013-.067.03-.092V3.81zm3.987-.34c0 .045.01.084.021.123.042.16.094.318.124.48.024.133.023.27.028.406 0 .033-.019.067-.032.11-.094-.058-.047-.158-.106-.215h-.125c-.015.072-.01.152-.046.2-.066.085-.155.154-.236.227-.043.038-.078.018-.103-.025l-.046-.087c-.065.035-.117.069-.172.093-.116.051-.235.095-.35.147-.085.038-.09.053-.07.147.014.075.034.148.047.223.013.072.05.109.123.124.233.05.462.115.657.265.058-.102.058-.102.168-.151.03-.014.06-.03.092-.042.08-.03.115-.017.15.06.023.048.041.098.066.158.06-.14-.042-.267.017-.416.157.18.24.39.375.567a.235.235 0 00.022-.098c.002-.124 0-.247.002-.371 0-.034.013-.067.02-.1l.032-.003c.11.155.13.354.226.52a3.036 3.036 0 00-.01-.392c-.004-.045 0-.074.05-.088.08.036.116.14.215.158-.03-.275-.423-1.137-.798-1.635-.114-.127-.2-.28-.34-.386zm-2.667.696c-.019.034-.03.05-.037.067-.061.185-.125.37-.18.556-.031.105-.087.169-.195.19-.09.019-.178.052-.268.073-.038.009-.089.015-.118-.003-.024-.016-.025-.069-.036-.106-.064.076-.082.087-.17.047-.133-.062-.262-.135-.393-.201-.048-.025-.093-.063-.17-.03-.043.12-.091.25-.137.382-.099.28-.087.242.095.453.046.048.102.03.154.023.054-.009.106-.03.16-.036.13-.013.26-.08.367-.015.204-.064.387-.122.571-.178.05-.015.089.005.114.054.022.042.034.093.082.121.038-.056-.013-.128.063-.178l.14.241-.042-1.46zm.278.358c-.096-.01-.107.01-.11.108-.002.038-.003.078.002.115.03.2.099.386.174.57.002.006.012.01.022.015l.078-.05c.052.036.081.088.153.088.205-.002.41.014.616.012.099-.001.158.042.205.12.018.03.024.077.088.066l-.08-.394c-.05-.195-.085-.395-.172-.589-.057.057-.114.068-.18.046a.72.72 0 00-.135-.028c-.22-.028-.44-.059-.66-.08zm10.254-1.727c.089.163.155.316.139.491-.016.168.026.342-.044.516-.047-.033-.088-.082-.112-.075-.117.035-.164-.057-.227-.115a4.772 4.772 0 01-.286-.29l-.104-.113a4.856 4.856 0 01-.023.019c.035.046.07.093.11.156.04.064.084.127.122.193.034.058.065.118.031.205-.082-.01-.164-.019-.246-.032-.06-.01-.101 0-.124.07-.031.098-.037.096-.15.09.02.042.036.08.057.116.041.074.03.138-.03.196-.06.06-.118.122-.178.181a.175.175 0 01-.185.046c-.222-.061-.447-.113-.67-.174-.032-.009-.063-.04-.086-.068-.03-.04-.052-.087-.08-.13-.044-.07-.09-.138-.136-.207a.18.18 0 00-.014.105c.012.127.03.253.035.38.005.1-.024.12-.121.104-.104-.017-.206-.04-.31-.058-.064-.012-.131-.028-.202.03l.081.208c.09 0 .166-.01.237.002a.819.819 0 01.458.251c.078.083.154.168.241.26l.018-.005c-.004-.006-.008-.013-.01-.04.014-.056-.062-.118.018-.178.031.03.064.057.088.09.058.078.111.159.169.257l.089.141.024-.013a2093.819 2093.819 0 01-.427-.934c.055.007.083.007.108.016.193.07.385.142.577.216.074.028.147.06.219.094.062.028.112.018.157-.033.05-.056.102-.112.154-.167.05-.051.095-.046.132.014.016.025.026.053.04.08.071.138.143.277.217.433l.159.308.025-.011c-.044-.106-.07-.218-.138-.334-.057-.182-.168-.346-.206-.545.136.034.362.326.567.732l.057.074.018-.011a1.563 1.563 0 01-.052-.127c-.046-.145-.097-.29-.136-.436-.022-.083-.036-.173.022-.26l.109.058-.026-.207.027-.016c.022.02.05.036.065.06.073.108.143.22.215.33.01.016.029.029.043.043-.036-.217-.2-.38-.229-.626l.155.112c.014-.166.012-.319.042-.465.032-.158-.023-.297-.063-.445.024.004.036.006.055.025.092.124.183.249.277.371.02.027.05.047.069.087l.04.063.019-.015a.293.293 0 01-.053-.082 27.922 27.922 0 01-.332-.49c-.221-.311-.363-.467-.485-.521zm-6.57.327c-.003.161.092.275.069.415l-.368.087c.09.139.032.237-.052.331-.05.057-.092.122-.143.178-.037.04-.046.078-.018.126l.16.275c.029.048.072.066.128.064.076-.003.152 0 .228-.001.116-.003.216.022.275.137.006.014.02.024.044.052.004-.059-.003-.098.01-.13.016-.04.04-.099.072-.108.084-.023.173-.024.26-.03.013-.001.027.018.04.029l.071.065c.019-.11-.082-.198-.024-.31l.126.04c-.026-.123-.07-.245-.071-.366 0-.123.051-.243.115-.36.107.062.16.156.234.253.183.265.36.533.494.834.165-.078.27.068.407.088-.003-.106-.133-.441-.197-.492a.142.142 0 00-.102-.028c-.06.011-.119.039-.191.063-.025-.039-.056-.078-.077-.122a3.936 3.936 0 00-.473-.783c-.076-.094-.16-.182-.228-.26l-.391.285c-.049.035-.094.03-.132-.017l-.169-.207c-.025-.03-.053-.059-.097-.108z"
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
    ) : platform === "grok" ? (
      <GrokAgentSvg />
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

function statusDotClass(
  status: TerminalStatus,
  isActive: boolean,
  isDark: boolean,
  exitCode?: number | null,
) {
  if (status === "running") {
    return isActive ? "bg-emerald-400" : isDark ? "bg-[#5eead4]" : "bg-emerald-400";
  }
  if (status === "error") {
    return isActive ? "bg-red-500" : isDark ? "bg-[#fb7185]" : "bg-red-400";
  }
  // A non-zero exit code is a failed run, so it gets its own colour instead of the
  // neutral "finished cleanly" dot.
  if (status === "exited" && typeof exitCode === "number" && exitCode !== 0) {
    return isActive ? "bg-amber-500" : isDark ? "bg-[#fbbf24]" : "bg-amber-400";
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

function clampAudioVolume(value: number) {
  return Math.min(Math.max(value, 0), 1);
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
  onInteract?: () => void,
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
    return <StarshipHologram className={className} label={label} onInteract={onInteract} />;
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
  onHologramInteract,
  regionKeys,
}: {
  card: VibeSkinDecorationCard;
  onHologramInteract?: () => void;
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
      onHologramInteract,
    );
    const cardStatus = "status" in card ? card.status : "在线";

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
          {cardStatus && (
            <span className="vibe-skin-space-led shrink-0 rounded-full border px-2 py-0.5 text-[10px]">
              {cardStatus}
            </span>
          )}
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
    onHologramInteract,
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
  const [initialAppearance] = useState(() => readStoredVibeAppearance());
  const [tabs, setTabs] = useState<TerminalSession[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [createDialogOpen, setCreateDialogOpen] = useState(false);
  const [createProjectDir, setCreateProjectDir] = useState("");
  const [createPlatform, setCreatePlatform] = useState<AgentPlatform>("codex");
  const [launchPrompt, setLaunchPrompt] = useState("");
  const [launchModel, setLaunchModel] = useState<string>(autoLaunchOptionValue);
  const [launchReasoning, setLaunchReasoning] = useState<string>(autoLaunchOptionValue);
  const [installCommandCopied, setInstallCommandCopied] = useState(false);
  const [themeMode, setThemeMode] = useState<VibeTheme>(
    () => initialAppearance.themeMode ?? "dark",
  );
  const [skinAudioEnabled, setSkinAudioEnabled] = useState(
    () => initialAppearance.skinAudioEnabled ?? true,
  );
  const [skinAudioActivated, setSkinAudioActivated] = useState(false);
  const [appearanceOpen, setAppearanceOpen] = useState(false);
  const [startMenuOpen, setStartMenuOpen] = useState(false);
  const [clockNow, setClockNow] = useState(() => new Date());
  const [customSkin, setCustomSkin] = useState<VibeSkinDefinition | null>(() => readStoredVibeSkin());
  const [activeSkinId, setActiveSkinId] = useState<string>(
    () => initialAppearance.skinId ?? readStoredVibeSkin()?.id ?? BUILT_IN_VIBE_SKINS[0].id,
  );
  const [error, setError] = useState<string | null>(null);
  // The native directory picker comes from the Tauri dialog plugin, so in a
  // browser the entry points that use it have to be disabled rather than
  // rejecting into nothing.
  const desktop = isDesktop();
  const [sessionListScrolling, setSessionListScrolling] = useState(false);
  const [expandedDirectories, setExpandedDirectories] = useState<Set<string>>(() => new Set());
  const [sessionListCollapsed, setSessionListCollapsed] = useState(
    () => initialAppearance.sessionListCollapsed ?? false,
  );
  // Narrow windows swap the sidebar track for a floating drawer; that visibility is
  // transient so it must not overwrite the persisted wide-layout preference.
  const [narrowLayout, setNarrowLayout] = useState(
    () =>
      typeof window !== "undefined" && window.innerWidth < SESSION_LIST_DRAWER_BREAKPOINT,
  );
  const [sessionDrawerOpen, setSessionDrawerOpen] = useState(false);
  // `null` keeps the per-theme default width until the user drags the rail.
  const [sessionListWidth, setSessionListWidth] = useState<number | null>(() =>
    typeof initialAppearance.sessionListWidth === "number"
      ? clampSessionListWidth(initialAppearance.sessionListWidth)
      : null,
  );
  const [tiledTerminals, setTiledTerminals] = useState(
    () => initialAppearance.tiledTerminals ?? false,
  );
  const [tileWidth, setTileWidth] = useState(() =>
    typeof initialAppearance.tileWidth === "number"
      ? clampTileWidth(initialAppearance.tileWidth)
      : TILE_DEFAULT_WIDTH,
  );
  const [tabWidthFitsContent, setTabWidthFitsContent] = useState(
    () => initialAppearance.tabWidthFitsContent ?? false,
  );
  const [tabSettingsOpen, setTabSettingsOpen] = useState(false);
  // The tab strip scrolls horizontally once the tabs outgrow the workspace, so the
  // arrow buttons need to know which directions still have room.
  const [tabStripOverflow, setTabStripOverflow] = useState({ left: false, right: false });
  const [tabStripScrolling, setTabStripScrolling] = useState(false);
  // Exit codes only exist for tabs whose process died while Vibe was mounted.
  const [tabExitCodes, setTabExitCodes] = useState<Record<string, number>>({});
  const [tabsMenu, setTabsMenu] = useState<{ x: number; y: number } | null>(null);
  // Restored tabs are re-spawned from stored launch descriptors; persistence stays
  // paused until that pass finishes so an empty first render cannot wipe the list.
  const [tabsRestored, setTabsRestored] = useState(false);
  const skinFileInputRef = useRef<HTMLInputElement | null>(null);
  const startButtonRef = useRef<HTMLButtonElement | null>(null);
  const startMenuRef = useRef<HTMLDivElement | null>(null);
  const tabsMenuRef = useRef<HTMLDivElement | null>(null);
  const activeTileRef = useRef<HTMLDivElement | null>(null);
  const tabStripRef = useRef<HTMLDivElement | null>(null);
  const tabStripScrollTimeout = useRef<number | null>(null);
  const tabInputsRef = useRef(new Map<string, CreateTerminalSessionInput>());
  const tabRestoreStartedRef = useRef(false);
  const sessionListScrollTimeout = useRef<number | null>(null);
  const ambientAudioRef = useRef<AmbientAudioHandle[]>([]);
  const closingTabIdsRef = useRef(new Set<string>());

  const sessionsQuery = useQuery({
    queryKey: ["sessions"],
    queryFn: () => listSessions(null),
  });

  const agentLaunchOptionsQuery = useQuery({
    queryKey: ["agent-launch-options"],
    queryFn: () => listAgentLaunchOptions(),
  });

  const agentLaunchOptions = agentLaunchOptionsQuery.data ?? [];
  const activeAgentLaunchOption = useMemo(
    () => agentLaunchOptions.find((option) => option.platform === createPlatform) ?? null,
    [agentLaunchOptions, createPlatform],
  );
  const launchModelChoices = useMemo(
    () => activeAgentLaunchOption?.models ?? [],
    [activeAgentLaunchOption],
  );
  const selectedLaunchModel = useMemo(
    () => launchModelChoices.find((model) => model.id === launchModel) ?? null,
    [launchModel, launchModelChoices],
  );
  const launchReasoningChoices = useMemo(
    () => selectedLaunchModel?.reasoningLevels ?? [],
    [selectedLaunchModel],
  );
  const agentInstalled = activeAgentLaunchOption?.installed !== false;
  const agentInstallCommand = activeAgentLaunchOption?.installCommand ?? "";
  // A failed catalog fetch used to leave the agent panel silently empty, which
  // reads as a blank page. Surface it next to the agent strip instead.
  const agentCatalogError = agentLaunchOptionsQuery.isError
    ? t("vibe.errorAgentCatalog", { message: formatError(agentLaunchOptionsQuery.error) })
    : null;

  // Keep the model/reasoning selection valid whenever the agent changes or the
  // backend catalog no longer advertises the previously chosen value.
  useEffect(() => {
    if (
      launchModel !== autoLaunchOptionValue &&
      !launchModelChoices.some((model) => model.id === launchModel)
    ) {
      setLaunchModel(autoLaunchOptionValue);
    }
  }, [launchModel, launchModelChoices]);

  useEffect(() => {
    if (
      launchReasoning !== autoLaunchOptionValue &&
      !launchReasoningChoices.some((level) => level.effort === launchReasoning)
    ) {
      setLaunchReasoning(autoLaunchOptionValue);
    }
  }, [launchReasoning, launchReasoningChoices]);

  useEffect(() => {
    setInstallCommandCopied(false);
  }, [createPlatform]);

  const visibleSessions = useMemo(
    () => (sessionsQuery.data ?? []).filter((session) => !isBookkeepingSession(session)),
    [sessionsQuery.data],
  );
  const groupedSessions = useMemo(
    () => groupSessions(visibleSessions, t("vibe.unknownDirectory")),
    [visibleSessions, t],
  );
  const projectDirectories = useMemo(() => {
    const directories = new Set<string>();
    for (const session of visibleSessions) {
      const directory = session.projectDir?.trim();
      if (directory) {
        directories.add(directory);
      }
    }
    return Array.from(directories);
  }, [visibleSessions]);

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

  useEffect(() => {
    if (!availableSkins.some((skin) => skin.id === activeSkinId)) {
      setActiveSkinId(BUILT_IN_VIBE_SKINS[0].id);
    }
  }, [activeSkinId, availableSkins]);

  useEffect(() => {
    writeStoredVibeAppearance({
      themeMode,
      skinId: activeSkin.id,
      skinAudioEnabled,
      tiledTerminals,
      sessionListCollapsed,
      sessionListWidth: sessionListWidth ?? undefined,
      tileWidth,
      tabWidthFitsContent,
    });
  }, [
    activeSkin.id,
    sessionListCollapsed,
    sessionListWidth,
    skinAudioEnabled,
    tabWidthFitsContent,
    themeMode,
    tileWidth,
    tiledTerminals,
  ]);

  const openTerminal = useCallback(async (input: CreateTerminalSessionInput) => {
    setError(null);
    // The drawer covers the workspace, so opening a tab from it should reveal the terminal.
    setSessionDrawerOpen(false);
    try {
      const session = await createTerminalSession(input);
      tabInputsRef.current.set(session.id, input);
      setTabs((current) => [...current, session]);
      setActiveId(session.id);
    } catch (caught) {
      setError(formatError(caught));
    }
  }, []);

  // PTYs die with the process, so restoring a tab means re-spawning it from the
  // stored launch descriptor. This runs once, the first time Vibe mounts.
  useEffect(() => {
    if (tabRestoreStartedRef.current) {
      return;
    }
    tabRestoreStartedRef.current = true;

    const descriptors = readStoredVibeTabs();
    if (descriptors.length === 0) {
      setTabsRestored(true);
      return;
    }

    void (async () => {
      const restored: TerminalSession[] = [];
      let restoredActiveId: string | null = null;
      let firstFailure: string | null = null;

      for (const descriptor of descriptors) {
        try {
          const session = await createTerminalSession(descriptor.input);
          tabInputsRef.current.set(session.id, descriptor.input);
          restored.push(session);
          if (descriptor.active) {
            restoredActiveId = session.id;
          }
        } catch (caught) {
          // A folder can disappear between runs; keep restoring the rest.
          firstFailure = firstFailure ?? formatError(caught);
        }
      }

      if (restored.length > 0) {
        setTabs((current) => (current.length > 0 ? current : restored));
        setActiveId((current) => current ?? restoredActiveId ?? restored[0].id);
      }
      if (firstFailure) {
        setError(firstFailure);
      }
      setTabsRestored(true);
    })();
  }, []);

  useEffect(() => {
    if (!tabsRestored) {
      return;
    }

    const descriptors: VibeTabDescriptor[] = [];
    for (const tab of tabs) {
      const input = tabInputsRef.current.get(tab.id);
      if (!input) {
        continue;
      }
      descriptors.push({ input, active: tab.id === activeId });
    }
    writeStoredVibeTabs(descriptors);
  }, [activeId, tabs, tabsRestored]);

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
    if (!desktop) {
      return;
    }

    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: t("vibe.chooseFolder"),
      });
      if (typeof selected === "string") {
        setCreateProjectDir(selected);
      }
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : t("errors.operationFailed"));
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
    if (!agentInstalled) {
      setError(
        t("vibe.errorAgentNotInstalled", {
          agent: activeAgentLaunchOption?.displayName ?? createPlatform,
        }),
      );
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
      model: launchModel === autoLaunchOptionValue ? null : launchModel,
      reasoningEffort: launchReasoning === autoLaunchOptionValue ? null : launchReasoning,
    });
    setLaunchPrompt("");
    if (closeDialog) {
      setCreateDialogOpen(false);
    }
  };

  const copyInstallCommand = async () => {
    if (!agentInstallCommand) {
      return;
    }
    try {
      if (typeof navigator === "undefined" || !navigator.clipboard?.writeText) {
        throw new Error(t("vibe.errorClipboardUnavailable"));
      }
      await navigator.clipboard.writeText(agentInstallCommand);
      setInstallCommandCopied(true);
    } catch (caught) {
      setError(formatError(caught));
    }
  };

  const launchNewAgent = () => {
    launchAgentSession(true);
  };

  const launchFromComposer = () => {
    launchAgentSession(false);
  };

  const closeTab = async (session: TerminalSession) => {
    if (closingTabIdsRef.current.has(session.id)) {
      return;
    }
    closingTabIdsRef.current.add(session.id);
    setError(null);
    try {
      if (session.status === "running") {
        await killTerminalSession(session.id);
      }
      setTabs((current) => {
        const remaining = current.filter((tab) => tab.id !== session.id);
        tabInputsRef.current.delete(session.id);
        setActiveId((currentActive) => {
          if (currentActive !== session.id) {
            return currentActive;
          }
          return remaining[0]?.id ?? null;
        });
        return remaining;
      });
      setTabExitCodes((current) => {
        if (!(session.id in current)) {
          return current;
        }
        const next = { ...current };
        delete next[session.id];
        return next;
      });
    } catch (caught) {
      setError(formatError(caught));
    } finally {
      closingTabIdsRef.current.delete(session.id);
    }
  };

  const updateStatus = useCallback(
    (sessionId: string, status: TerminalStatus, exitCode?: number | null) => {
      setTabs((current) =>
        current.map((tab) => (tab.id === sessionId ? { ...tab, status } : tab)),
      );
      setTabExitCodes((current) => {
        const next = { ...current };
        if (status === "exited" && typeof exitCode === "number") {
          next[sessionId] = exitCode;
        } else {
          delete next[sessionId];
        }
        return next;
      });
    },
    [],
  );

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

  const openTabsMenu = useCallback((event: ReactMouseEvent<HTMLDivElement>) => {
    event.preventDefault();
    // The menu is absolutely positioned inside the workspace column, so translate the
    // pointer position into that container's coordinate space.
    const container = event.currentTarget.parentElement;
    const bounds = container?.getBoundingClientRect();
    setTabsMenu({
      x: Math.max(0, event.clientX - (bounds?.left ?? 0)),
      y: Math.max(0, event.clientY - (bounds?.top ?? 0)),
    });
  }, []);

  const toggleTiledTerminals = useCallback(() => {
    setTiledTerminals((current) => !current);
    setTabsMenu(null);
  }, []);

  // Clicking a tab in tiled mode should bring that tile into view instead of
  // swapping panes, since every terminal stays visible.
  useEffect(() => {
    if (!tiledTerminals) {
      return;
    }
    const tile = activeTileRef.current;
    if (typeof tile?.scrollIntoView === "function") {
      tile.scrollIntoView({ block: "nearest", inline: "nearest" });
    }
  }, [activeId, tiledTerminals]);

  useEffect(() => {
    if (!tabsMenu) {
      return;
    }

    const closeOnOutsideMouseDown = (event: MouseEvent) => {
      const target = event.target;
      if (target instanceof Node && tabsMenuRef.current?.contains(target)) {
        return;
      }
      setTabsMenu(null);
    };

    window.addEventListener("mousedown", closeOnOutsideMouseDown);
    return () => window.removeEventListener("mousedown", closeOnOutsideMouseDown);
  }, [tabsMenu]);

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

  const syncTabStripOverflow = useCallback(() => {
    const strip = tabStripRef.current;
    if (!strip) {
      return;
    }
    const maxScrollLeft = strip.scrollWidth - strip.clientWidth;
    // Sub-pixel layout rounding can leave a fraction of a pixel behind, which would
    // otherwise keep an arrow enabled with nothing left to scroll.
    const next = {
      left: strip.scrollLeft > 1,
      right: maxScrollLeft - strip.scrollLeft > 1,
    };
    setTabStripOverflow((current) =>
      current.left === next.left && current.right === next.right ? current : next,
    );
  }, []);

  const markTabStripScrolling = useCallback(() => {
    if (tabStripScrollTimeout.current !== null) {
      window.clearTimeout(tabStripScrollTimeout.current);
    }

    setTabStripScrolling(true);
    tabStripScrollTimeout.current = window.setTimeout(() => {
      setTabStripScrolling(false);
      tabStripScrollTimeout.current = null;
    }, 800);
  }, []);

  const handleTabStripScroll = useCallback(() => {
    markTabStripScrolling();
    syncTabStripOverflow();
  }, [markTabStripScrolling, syncTabStripOverflow]);

  const scrollTabStrip = useCallback(
    (direction: -1 | 1) => {
      const strip = tabStripRef.current;
      if (!strip) {
        return;
      }
      const step = Math.max(160, Math.round(strip.clientWidth * 0.7));
      const amount = step * direction;
      if (typeof strip.scrollBy === "function") {
        strip.scrollBy({ behavior: "smooth", left: amount });
      } else {
        strip.scrollLeft += amount;
      }
      markTabStripScrolling();
      // Smooth scrolling settles asynchronously, so recheck once the frame lands.
      window.requestAnimationFrame(syncTabStripOverflow);
    },
    [markTabStripScrolling, syncTabStripOverflow],
  );

  useEffect(() => {
    syncTabStripOverflow();
  }, [narrowLayout, sessionListCollapsed, syncTabStripOverflow, tabWidthFitsContent, tabs]);

  useEffect(() => {
    const strip = tabStripRef.current;
    window.addEventListener("resize", syncTabStripOverflow);
    const observer =
      typeof ResizeObserver === "undefined" || !strip
        ? null
        : new ResizeObserver(() => syncTabStripOverflow());
    observer?.observe(strip as Element);
    return () => {
      window.removeEventListener("resize", syncTabStripOverflow);
      observer?.disconnect();
    };
  }, [syncTabStripOverflow]);

  useEffect(() => {
    return () => {
      if (tabStripScrollTimeout.current !== null) {
        window.clearTimeout(tabStripScrollTimeout.current);
      }
    };
  }, []);

  useEffect(() => {
    const syncNarrowLayout = () =>
      setNarrowLayout(window.innerWidth < SESSION_LIST_DRAWER_BREAKPOINT);
    syncNarrowLayout();
    window.addEventListener("resize", syncNarrowLayout);
    return () => window.removeEventListener("resize", syncNarrowLayout);
  }, []);

  useEffect(() => {
    if (!narrowLayout && sessionDrawerOpen) {
      setSessionDrawerOpen(false);
    }
  }, [narrowLayout, sessionDrawerOpen]);

  useEffect(() => {
    if (!sessionDrawerOpen) {
      return;
    }
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setSessionDrawerOpen(false);
      }
    };
    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [sessionDrawerOpen]);

  const activeTab = tabs.find((tab) => tab.id === activeId) ?? null;
  const isDark = themeMode === "dark";
  const isSkin = themeMode === "skin";
  const tabTooltip = useCallback(
    (tab: TerminalSession) => {
      const exitCode = tabExitCodes[tab.id];
      const status =
        tab.status === "exited" && typeof exitCode === "number"
          ? `${statusLabel(tab.status, t)} · ${t("vibe.statusExitCode", { code: String(exitCode) })}`
          : statusLabel(tab.status, t);
      return `${tab.title} · ${status}`;
    },
    [t, tabExitCodes],
  );
  const decorations = activeSkin.decorations;
  const skinVariant = skinVariantClass(isSkin ? decorations?.variant : undefined);
  const isStarshipSkin = Boolean(isSkin && decorations?.variant === "starship-cockpit");
  const skinStyle = useMemo(
    () => (isSkin ? skinToCssVariables(activeSkin) : undefined),
    [activeSkin, isSkin],
  );
  const effectiveSessionListWidth =
    sessionListWidth ??
    (isSkin ? SESSION_LIST_SKIN_DEFAULT_WIDTH : SESSION_LIST_DEFAULT_WIDTH);
  const rootStyle = useMemo(
    () =>
      ({
        ...(skinStyle ?? {}),
        "--vibe-session-list-width": `${effectiveSessionListWidth}px`,
        "--vibe-tile-width": `${tileWidth}px`,
      }) as CSSProperties,
    [effectiveSessionListWidth, skinStyle, tileWidth],
  );
  const { dragging: sessionListResizing, startDragging: startSessionListResize } = useDragResize({
    axis: "x",
    min: SESSION_LIST_MIN_WIDTH,
    max: SESSION_LIST_MAX_WIDTH,
    getInitialValue: () => effectiveSessionListWidth,
    onChange: (value) => setSessionListWidth(clampSessionListWidth(value)),
  });
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
  // The session list only owns a grid track on wide windows. Narrow windows keep the
  // rail plus workspace columns and render the list as an out-of-flow drawer, which
  // avoids the old single-column fallback that stacked the list above the terminal.
  const sessionListInTrack = !narrowLayout && !sessionListCollapsed;
  const sessionDrawerVisible = narrowLayout && sessionDrawerOpen;
  const sessionListVisible = sessionListInTrack || sessionDrawerVisible;
  const bodyGridColumnsClass =
    showSkinRightRail && !narrowLayout
      ? sessionListInTrack
        ? "grid-cols-[var(--vibe-session-list-width)_20px_minmax(0,1fr)_260px]"
        : "grid-cols-[20px_minmax(0,1fr)_260px]"
      : sessionListInTrack
        ? "grid-cols-[var(--vibe-session-list-width)_20px_minmax(0,1fr)]"
        : "grid-cols-[20px_minmax(0,1fr)]";
  const skinBodyGridClass = `vibe-skin-body grid min-h-0 flex-1 ${bodyGridColumnsClass}`;
  const plainBodyGridClass = `grid h-full min-h-0 ${bodyGridColumnsClass}`;
  const activeSkinRegionKeys = isSkin
    ? VIBE_SKIN_REGION_KEYS.filter((region) => Boolean(activeSkin.regions?.[region]))
    : [];
  const taskbarEnabled = Boolean(isSkin && skinBlocks.taskbar.enabled);
  const currentTime = formatTaskbarClock(clockNow);

  const stopAmbientAudio = useCallback(() => {
    for (const handle of ambientAudioRef.current) {
      if (handle.intervalId !== undefined) {
        window.clearInterval(handle.intervalId);
      }
      handle.audio.pause();
      handle.audio.currentTime = 0;
    }
    ambientAudioRef.current = [];
  }, []);

  const activateSkinAudio = useCallback(() => {
    setSkinAudioActivated(true);
  }, []);

  const playSkinAudioEvent = useCallback(
    (eventName: VibeSkinAudioEvent) => {
      if (!isSkin || !skinAudioEnabled || activeSkin.audio?.enabled === false) {
        return;
      }

      const src = activeSkin.audio?.events?.[eventName];
      if (!src || typeof Audio === "undefined") {
        return;
      }

      const audio = new Audio(src);
      audio.volume = clampAudioVolume(activeSkin.audio?.volume ?? 0.5);
      void audio.play().catch(() => undefined);
    },
    [activeSkin.audio, isSkin, skinAudioEnabled],
  );

  const handleHologramInteract = useCallback(() => {
    playSkinAudioEvent("hologramInteract");
  }, [playSkinAudioEvent]);

  useEffect(() => {
    stopAmbientAudio();

    if (
      !skinAudioActivated ||
      !isSkin ||
      !skinAudioEnabled ||
      activeSkin.audio?.enabled === false ||
      typeof Audio === "undefined"
    ) {
      return;
    }

    for (const item of activeSkin.audio?.ambient ?? []) {
      const audio = new Audio(item.src);
      audio.loop = Boolean(item.loop);
      audio.volume = clampAudioVolume(item.volume ?? activeSkin.audio?.volume ?? 0.35);

      const play = () => {
        audio.currentTime = 0;
        void audio.play().catch(() => undefined);
      };

      const handle: AmbientAudioHandle = { audio };
      if (item.loop) {
        play();
      } else if (item.intervalMs) {
        play();
        handle.intervalId = window.setInterval(play, item.intervalMs);
      } else {
        play();
      }
      ambientAudioRef.current.push(handle);
    }

    return stopAmbientAudio;
  }, [
    activeSkin.audio,
    isSkin,
    skinAudioActivated,
    skinAudioEnabled,
    stopAmbientAudio,
  ]);

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
    ? "vibe-skin-launch-panel mx-auto w-full max-w-4xl shrink-0 rounded-[1.1rem] border p-2 text-left shadow-2xl backdrop-blur-xl"
    : isDark
      ? "mx-auto w-full max-w-4xl shrink-0 rounded-[1.5rem] border border-[#073642] bg-[#073642]/70 p-3 text-left shadow-2xl shadow-black/30 backdrop-blur-xl sm:p-4"
      : "mx-auto w-full max-w-4xl shrink-0 rounded-[1.5rem] border border-white/80 bg-white/82 p-3 text-left shadow-2xl shadow-stone-900/10 backdrop-blur-xl sm:p-4";
  const agentStripClass = isSkin
    ? "vibe-skin-agent-strip flex flex-wrap items-center gap-1.5 rounded-lg border px-1.5 py-1"
    : isDark
      ? "rounded-2xl border border-[#586e75]/55 bg-[#002b36]/72 p-2 text-[#d8e2dc]"
      : "rounded-2xl border border-stone-200 bg-white/78 p-2 text-stone-700";
  const agentOptionClass = (active: boolean) =>
    isSkin
      ? `vibe-skin-agent-option ${
          active ? "vibe-skin-agent-option-active" : ""
        } inline-flex items-center gap-1.5 rounded-md border px-1.5 py-1 text-[10px] font-semibold leading-none motion-control`
      : isDark
        ? `inline-flex items-center gap-2 rounded-xl border px-3 py-2 text-[12px] font-semibold motion-control ${
            active
              ? "border-[#2aa198] bg-[#2aa198]/18 text-[#fdf6e3] shadow-[0_0_18px_rgba(42,161,152,0.18)]"
              : "border-[#586e75]/50 bg-[#073642]/64 text-[#9fc3cf] hover:border-[#839496] hover:text-[#fdf6e3]"
          }`
        : `inline-flex items-center gap-2 rounded-xl border px-3 py-2 text-[12px] font-semibold motion-control ${
            active
              ? "border-emerald-300 bg-emerald-50 text-emerald-900 shadow-sm"
              : "border-stone-200 bg-white/70 text-stone-600 hover:border-stone-300 hover:text-stone-950"
          }`;
  const composerClass = isSkin
    ? "vibe-skin-composer mt-1.5 rounded-lg border p-1.5"
    : isDark
      ? "mt-3 rounded-2xl border border-[#586e75]/50 bg-[#001e27]/72 p-2"
      : "mt-3 rounded-2xl border border-stone-200 bg-stone-50/82 p-2";
  const composerInputClass = isSkin
    ? "vibe-skin-composer-input min-h-[3.25rem] w-full resize-none rounded-md border px-2 py-1.5 text-[12px] outline-none motion-control"
    : isDark
      ? "min-h-20 w-full resize-none rounded-xl border border-[#586e75]/50 bg-[#002b36]/70 px-3 py-3 text-sm text-[#fdf6e3] outline-none placeholder:text-[#586e75] focus:border-[#268bd2]"
      : "min-h-20 w-full resize-none rounded-xl border border-stone-200 bg-white px-3 py-3 text-sm text-stone-950 outline-none placeholder:text-stone-400 focus:border-blue-400";
  const composerMetaBarClass = isSkin
    ? "vibe-skin-composer-meta-bar mt-1.5 flex flex-col gap-1 rounded-md border p-1 sm:flex-row sm:items-center"
    : isDark
      ? "mt-2 flex flex-col gap-2 rounded-xl border border-[#586e75]/40 bg-[#073642]/50 p-2 sm:flex-row sm:items-end"
      : "mt-2 flex flex-col gap-2 rounded-xl border border-stone-200 bg-white/70 p-2 sm:flex-row sm:items-end";
  const composerLabelClass = isSkin
    ? "min-w-0 flex flex-1 items-center gap-1 text-[10px] font-semibold leading-none"
    : "min-w-0 flex-1 text-[11px] font-semibold";
  const composerLabelTextClass = isSkin
    ? "shrink-0 whitespace-nowrap text-[var(--vibe-muted-text)]"
    : "";
  const composerControlClass = isSkin
    ? "vibe-skin-composer-control h-7 w-full min-w-0 flex-1 rounded-md border px-1.5 text-[10px] outline-none motion-control"
    : isDark
      ? "mt-1 h-10 w-full min-w-0 rounded-xl border border-[#586e75]/55 bg-[#002b36] px-3 text-[12px] text-[#fdf6e3] outline-none focus:border-[#268bd2]"
      : "mt-1 h-10 w-full min-w-0 rounded-xl border border-stone-200 bg-white px-3 text-[12px] text-stone-950 outline-none focus:border-blue-400";
  const composerSendButtonClass = isSkin
    ? "vibe-skin-composer-send-button inline-flex h-7 shrink-0 items-center justify-center gap-1.5 rounded-md border px-2.5 text-[11px] font-semibold motion-control sm:ml-auto"
    : isDark
      ? "inline-flex h-10 shrink-0 items-center justify-center gap-2 rounded-xl border border-[#b58900] bg-[#b58900] px-4 text-[13px] font-semibold text-[#002b36] motion-control hover:bg-[#cb4b16] hover:text-white sm:ml-auto"
      : "inline-flex h-10 shrink-0 items-center justify-center gap-2 rounded-xl bg-stone-950 px-4 text-[13px] font-semibold text-white motion-control hover:bg-stone-800 sm:ml-auto";
  const composerAddonClass = isSkin
    ? "vibe-skin-composer-addon inline-flex shrink-0 items-center gap-1 rounded border px-1.5 py-0.5 text-[10px] font-semibold leading-none"
    : isDark
      ? "inline-flex shrink-0 items-center gap-1 rounded-full border border-[#586e75]/50 px-3 py-1 text-[11px] font-semibold text-[#9fc3cf]"
      : "inline-flex shrink-0 items-center gap-1 rounded-full border border-stone-200 px-3 py-1 text-[11px] font-semibold text-stone-500";
  const agentCatalogNotice = agentCatalogError ? (
    <div
      className={
        isSkin
          ? "vibe-skin-agent-strip mt-1 rounded-md border p-1.5 text-[10px]"
          : isDark
            ? "mt-2 rounded-xl border border-[#dc322f]/60 bg-[#dc322f]/12 p-2 text-[12px] text-[#eee8d5]"
            : "mt-2 rounded-xl border border-red-300 bg-red-50 p-2 text-[12px] text-red-900"
      }
      data-testid="vibe-agent-catalog-error"
      role="status"
    >
      {agentCatalogError}
    </div>
  ) : null;
  const agentMissingNotice = activeAgentLaunchOption && !activeAgentLaunchOption.installed ? (
    <div
      className={
        isSkin
          ? "vibe-skin-agent-strip mt-1 rounded-md border p-1.5 text-[10px]"
          : isDark
            ? "mt-2 rounded-xl border border-[#cb4b16]/60 bg-[#cb4b16]/12 p-2 text-[12px] text-[#eee8d5]"
            : "mt-2 rounded-xl border border-amber-300 bg-amber-50 p-2 text-[12px] text-amber-900"
      }
      role="status"
    >
      <p className="font-semibold">
        {t("vibe.agentNotInstalled", { agent: activeAgentLaunchOption.displayName })}
      </p>
      <div className="mt-1 flex flex-wrap items-center gap-2">
        <code className={isSkin ? "vibe-skin-composer-addon rounded border px-1.5 py-0.5" : "rounded bg-black/10 px-2 py-1 font-mono text-[11px]"}>
          {activeAgentLaunchOption.installCommand}
        </code>
        <button
          className={
            isSkin
              ? "vibe-skin-ghost rounded border px-2 py-0.5 text-[10px] font-semibold motion-control"
              : isDark
                ? "rounded-lg border border-[#586e75] px-2 py-1 text-[11px] font-semibold text-[#fdf6e3] motion-control hover:border-[#839496]"
                : "rounded-lg border border-amber-400 bg-white px-2 py-1 text-[11px] font-semibold text-amber-900 motion-control hover:border-amber-500"
          }
          onClick={() => void copyInstallCommand()}
          type="button"
        >
          {installCommandCopied ? t("vibe.installCommandCopied") : t("vibe.copyInstallCommand")}
        </button>
      </div>
    </div>
  ) : null;

  return (
    <main
      className={
        isSkin
          ? `vibe-skin ${skinVariant} h-screen max-h-[100dvh] overflow-hidden text-[var(--vibe-text)]`
          : isDark
            ? "h-screen max-h-[100dvh] overflow-hidden bg-[#002b36] text-[#d8e2dc]"
            : "h-screen max-h-[100dvh] overflow-hidden text-stone-950"
      }
      onKeyDownCapture={activateSkinAudio}
      onPointerDownCapture={activateSkinAudio}
      style={rootStyle}
    >
      <div
        className={isSkin ? "vibe-skin-frame flex h-full min-h-0 flex-col" : plainBodyGridClass}
        data-testid={isSkin ? undefined : "vibe-body-grid"}
      >
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
        {sessionDrawerVisible && (
          <div
            aria-hidden="true"
            className="vibe-session-drawer-backdrop"
            data-testid="vibe-session-drawer-backdrop"
            onClick={() => setSessionDrawerOpen(false)}
          />
        )}
        {sessionListVisible && (
        <aside
          className={`${
            isSkin
              ? "vibe-skin-sidebar relative flex h-full min-h-0 flex-col overflow-hidden border-r p-3 shadow-2xl"
              : isDark
              ? "relative flex h-full min-h-0 flex-col overflow-hidden border-r border-[#073642] bg-[#002b36] p-3 shadow-2xl shadow-black/25"
              : "relative flex h-full min-h-0 flex-col overflow-hidden border-r border-white/70 bg-gradient-to-br from-slate-50/92 via-emerald-50/74 to-amber-50/70 p-3 shadow-xl shadow-stone-900/5 backdrop-blur-2xl"
          }${sessionDrawerVisible ? " vibe-session-drawer" : ""}`}
          data-testid="vibe-session-list"
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
                    className="vibe-skin-ghost grid h-8 w-8 shrink-0 place-items-center rounded-xl border shadow-sm motion-control focus:outline-none focus-visible:ring-2"
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
                        ? "grid h-8 w-8 shrink-0 place-items-center rounded-xl border border-[#586e75] bg-[#073642] text-[#93a1a1] shadow-sm motion-control hover:border-[#839496] hover:text-[#fdf6e3] focus:outline-none focus-visible:ring-2 focus-visible:ring-[#268bd2]"
                        : "grid h-8 w-8 shrink-0 place-items-center rounded-xl border border-stone-200 bg-white/70 text-stone-600 shadow-sm motion-control hover:border-stone-300 hover:bg-white hover:text-stone-950 focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-400"
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
                    ? "vibe-skin-primary inline-flex flex-1 items-center justify-center gap-2 rounded-xl border px-3 py-2 text-[13px] font-semibold motion-control"
                    : isDark
                    ? "inline-flex flex-1 items-center justify-center gap-2 rounded-xl border border-[#b58900] bg-[#b58900] px-3 py-2 text-[13px] font-semibold text-[#002b36] motion-control hover:bg-[#cb4b16] hover:text-white"
                    : "inline-flex flex-1 items-center justify-center gap-2 rounded-xl bg-stone-950 px-3 py-2 text-[13px] font-semibold text-white motion-control hover:bg-stone-800"
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
                    ? "vibe-skin-ghost inline-flex h-8 shrink-0 items-center gap-1.5 rounded-xl border px-3.5 text-[12px] font-semibold motion-control"
                    : isDark
                    ? "inline-flex h-9 shrink-0 items-center gap-1.5 rounded-xl border border-[#586e75] bg-[#002b36] px-2 text-[12px] font-semibold text-[#fdf6e3] motion-control hover:border-[#839496] hover:bg-[#073642]"
                    : "inline-flex h-9 shrink-0 items-center gap-1.5 rounded-xl border border-stone-200 bg-white px-2 text-[12px] font-semibold text-stone-700 motion-control hover:border-stone-300 hover:bg-stone-50"
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
                          ? "vibe-skin-list-trigger flex w-full items-center gap-2 rounded-xl px-1 py-1 text-left text-[12px] font-semibold motion-control"
                          : isDark
                            ? "vibe-dark-list-trigger flex w-full items-center gap-2 rounded-xl px-1 py-1 text-left text-[12px] font-semibold motion-control"
                            : "vibe-light-list-trigger flex w-full items-center gap-2 rounded-xl px-1 py-1 text-left text-[12px] font-semibold motion-control"
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
                                  ? "vibe-skin-session w-full rounded-xl border px-3 py-2 text-left text-[13px] motion-control disabled:cursor-not-allowed disabled:opacity-45"
                                  : isDark
                                    ? "vibe-dark-session-card w-full rounded-xl border px-3 py-2 text-left text-[13px] motion-control disabled:cursor-not-allowed disabled:opacity-55"
                                    : "vibe-light-session-card w-full rounded-xl border px-3 py-2 text-left text-[13px] motion-control disabled:cursor-not-allowed disabled:opacity-45"
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
        )}

        <div
          className={
            isSkin
              ? "vibe-session-rail vibe-skin-session-rail flex"
              : isDark
                ? "vibe-session-rail vibe-dark-session-rail flex"
                : "vibe-session-rail vibe-light-session-rail flex"
          }
        >
          {sessionListInTrack && (
            <div
              aria-label={t("vibe.resizeSessionList")}
              aria-orientation="vertical"
              aria-valuemax={SESSION_LIST_MAX_WIDTH}
              aria-valuemin={SESSION_LIST_MIN_WIDTH}
              aria-valuenow={effectiveSessionListWidth}
              className={`vibe-session-rail-drag ${
                sessionListResizing ? "vibe-session-rail-drag-active" : ""
              }`}
              data-testid="vibe-session-resize-handle"
              onPointerDown={startSessionListResize}
              role="separator"
              title={t("vibe.resizeSessionList")}
            />
          )}
          <button
            aria-expanded={sessionListVisible}
            aria-label={
              sessionListVisible ? t("vibe.collapseSessionList") : t("vibe.expandSessionList")
            }
            className="vibe-session-rail-handle"
            onClick={() => {
              if (narrowLayout) {
                setSessionDrawerOpen((current) => !current);
                return;
              }
              setSessionListCollapsed((current) => !current);
            }}
            title={
              sessionListVisible ? t("vibe.collapseSessionList") : t("vibe.expandSessionList")
            }
            type="button"
          >
            {sessionListVisible ? (
              <ChevronLeft className="h-3 w-3" />
            ) : (
              <ChevronRight className="h-3 w-3" />
            )}
          </button>
          {sessionListInTrack && (
            <div
              aria-hidden="true"
              className={`vibe-session-rail-drag ${
                sessionListResizing ? "vibe-session-rail-drag-active" : ""
              }`}
              onPointerDown={startSessionListResize}
            />
          )}
        </div>

        <div
          className={
            isSkin
              ? "vibe-skin-workspace relative flex h-full min-h-0 min-w-0 flex-col overflow-hidden shadow-xl"
              : isDark
                ? "relative flex h-full min-h-0 min-w-0 flex-col overflow-hidden bg-[#002b36] shadow-xl shadow-black/20"
                : "vibe-light-workspace relative flex h-full min-h-0 min-w-0 flex-col overflow-hidden bg-zinc-100 shadow-xl shadow-stone-900/5"
          }
        >
          <div
            className={
              isSkin
                ? `vibe-scrollbar ${scrollbarThemeClass} vibe-scrollbar-horizontal vibe-skin-tabbar ${tabStripScrolling ? "vibe-scrollbar-active" : ""} flex h-10 shrink-0 items-stretch gap-0 overflow-x-auto border-b px-1`
                : isDark
                  ? `vibe-scrollbar vibe-scrollbar-dark vibe-scrollbar-horizontal vibe-dark-tabbar ${tabStripScrolling ? "vibe-scrollbar-active" : ""} flex h-10 shrink-0 items-stretch gap-1 overflow-x-auto border-b px-1`
                  : `vibe-scrollbar vibe-scrollbar-light vibe-scrollbar-horizontal vibe-light-tabbar ${tabStripScrolling ? "vibe-scrollbar-active" : ""} flex h-10 shrink-0 items-stretch gap-1 overflow-x-auto border-b px-1`
            }
            data-testid="vibe-tab-strip"
            onContextMenu={openTabsMenu}
            onScroll={handleTabStripScroll}
            ref={tabStripRef}
          >
            {tabs.length === 0 && (
              <p className={isSkin ? "flex items-center px-3 text-[12px] text-[var(--vibe-muted-text)]" : isDark ? "flex items-center px-3 text-[12px] text-[#9fc3cf]" : "flex items-center px-3 text-[12px] text-stone-500"}>
                {t("vibe.noTabs")}
              </p>
            )}
            {tabs.map((tab) => (
              <div
                className={`group relative inline-flex h-full max-w-[220px] shrink-0 items-center overflow-hidden border-r ${
                  tabWidthFitsContent ? "" : "min-w-[132px]"
                } ${
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
                title={tabTooltip(tab)}
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
                  className="vibe-tab-trigger inline-flex h-full min-w-0 flex-1 items-center gap-2 bg-transparent px-3 pr-1 text-[12px] font-medium"
                  onClick={() => setActiveId(tab.id)}
                  type="button"
                >
                  <span
                    className={`h-1.5 w-1.5 shrink-0 rounded-full ${statusDotClass(tab.status, activeId === tab.id, isDark, tabExitCodes[tab.id])}`}
                    data-testid={`vibe-tab-status-${tab.id}`}
                  />
                  <span className="truncate">{shortTabTitle(tab.title)}</span>
                </button>
                <button
                  aria-label={t("vibe.closeTabAria", { title: tab.title })}
                  className={
                    isSkin
                      ? "vibe-skin-tab-close vibe-tab-close-icon inline-flex h-full shrink-0 items-center justify-center opacity-60 motion-control group-hover:opacity-100"
                      : isDark
                        ? "vibe-dark-tab-close vibe-tab-close-icon inline-flex h-full shrink-0 items-center justify-center opacity-60 motion-control group-hover:opacity-100"
                        : "vibe-light-tab-close vibe-tab-close-icon inline-flex h-full shrink-0 items-center justify-center opacity-60 motion-control group-hover:opacity-100"
                  }
                  onClick={() => void closeTab(tab)}
                  type="button"
                >
                  <X className="h-3 w-3" />
                </button>
              </div>
            ))}
            {/* Sticky so the control stays reachable once the tab strip overflows. */}
            <div
              className={
                isSkin
                  ? "vibe-skin-tabbar-actions sticky right-0 ml-auto flex shrink-0 items-center pl-2"
                  : isDark
                    ? "vibe-dark-tabbar-actions sticky right-0 ml-auto flex shrink-0 items-center pl-2"
                    : "vibe-light-tabbar-actions sticky right-0 ml-auto flex shrink-0 items-center pl-2"
              }
            >
              <button
                aria-label={t("vibe.scrollTabsLeft")}
                className={
                  isSkin
                    ? "vibe-skin-ghost mr-1 grid h-7 w-7 place-items-center rounded-lg border motion-control disabled:opacity-40"
                    : isDark
                      ? "mr-1 grid h-7 w-7 place-items-center rounded-lg border border-[#586e75] text-[#93a1a1] motion-control hover:text-[#fdf6e3] disabled:opacity-40 disabled:hover:text-[#93a1a1]"
                      : "mr-1 grid h-7 w-7 place-items-center rounded-lg border border-stone-200 text-stone-500 motion-control hover:text-stone-950 disabled:opacity-40 disabled:hover:text-stone-500"
                }
                disabled={!tabStripOverflow.left}
                onClick={() => scrollTabStrip(-1)}
                type="button"
              >
                <ChevronLeft className="h-3.5 w-3.5" />
              </button>
              <button
                aria-label={t("vibe.scrollTabsRight")}
                className={
                  isSkin
                    ? "vibe-skin-ghost mr-1 grid h-7 w-7 place-items-center rounded-lg border motion-control disabled:opacity-40"
                    : isDark
                      ? "mr-1 grid h-7 w-7 place-items-center rounded-lg border border-[#586e75] text-[#93a1a1] motion-control hover:text-[#fdf6e3] disabled:opacity-40 disabled:hover:text-[#93a1a1]"
                      : "mr-1 grid h-7 w-7 place-items-center rounded-lg border border-stone-200 text-stone-500 motion-control hover:text-stone-950 disabled:opacity-40 disabled:hover:text-stone-500"
                }
                disabled={!tabStripOverflow.right}
                onClick={() => scrollTabStrip(1)}
                type="button"
              >
                <ChevronRight className="h-3.5 w-3.5" />
              </button>
              <button
                aria-label={t("vibe.tabSettings")}
                className={
                  isSkin
                    ? "vibe-skin-ghost grid h-7 w-7 place-items-center rounded-lg border motion-control"
                    : isDark
                      ? "grid h-7 w-7 place-items-center rounded-lg border border-[#586e75] text-[#93a1a1] motion-control hover:text-[#fdf6e3]"
                      : "grid h-7 w-7 place-items-center rounded-lg border border-stone-200 text-stone-500 motion-control hover:text-stone-950"
                }
                onClick={() => setTabSettingsOpen(true)}
                type="button"
              >
                <Settings2 className="h-3.5 w-3.5" />
              </button>
            </div>
          </div>

          {tabsMenu && (
            <div className="absolute z-40" ref={tabsMenuRef} style={{ left: tabsMenu.x, top: tabsMenu.y }}>
              <MotionMenu
                ariaLabel={t("vibe.tabsMenu")}
                className={
                  isSkin
                    ? "vibe-skin-panel-strong min-w-[11rem] overflow-hidden rounded-xl border p-1 shadow-xl"
                    : isDark
                      ? "min-w-[11rem] overflow-hidden rounded-xl border border-[#586e75] bg-[#073642] p-1 text-[#fdf6e3] shadow-xl shadow-black/40"
                      : "min-w-[11rem] overflow-hidden rounded-xl border border-stone-200 bg-white p-1 text-stone-950 shadow-xl shadow-stone-900/10"
                }
                open={Boolean(tabsMenu)}
                origin="top-left"
                role="menu"
              >
              <button
                className={
                  isSkin
                    ? "vibe-skin-taskbar-menu-item flex w-full items-center rounded-lg px-3 py-2 text-left text-[12px] motion-control"
                    : isDark
                      ? "flex w-full items-center rounded-lg px-3 py-2 text-left text-[12px] motion-control hover:bg-[#002b36]"
                      : "flex w-full items-center rounded-lg px-3 py-2 text-left text-[12px] motion-control hover:bg-stone-100"
                }
                onClick={toggleTiledTerminals}
                role="menuitem"
                type="button"
              >
                <Columns3 className="mr-2 h-3.5 w-3.5 shrink-0" />
                {tiledTerminals ? t("vibe.disableTiled") : t("vibe.enableTiled")}
              </button>
              </MotionMenu>
            </div>
          )}

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
                <div
                  className={
                    isSkin
                      ? "flex min-h-[4rem] flex-1 items-center justify-center pt-1"
                      : "flex min-h-[9rem] flex-1 items-center justify-center pt-4"
                  }
                >
                  <div>
                    <TerminalSquare
                      className={
                        isSkin
                          ? "mx-auto h-5 w-5 text-[var(--vibe-accent)]"
                          : isDark
                            ? "mx-auto h-8 w-8 text-[#586e75]"
                            : "mx-auto h-8 w-8 text-stone-400"
                      }
                    />
                    <p
                      className={
                        isSkin
                          ? "mt-1 text-[11px] font-semibold leading-none text-[var(--vibe-text)]"
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
                          ? "mt-0.5 text-[10px] leading-tight text-[var(--vibe-muted-text)]"
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
                    <div
                      className={
                        isSkin
                          ? "flex min-w-0 shrink-0 items-center gap-1"
                          : "mb-2 flex flex-wrap items-center justify-between gap-2"
                      }
                    >
                      <div className={isSkin ? "flex min-w-0 items-center gap-1.5" : "flex min-w-0 items-center gap-2"}>
                        {launchAgentPrefix && <span className={composerAddonClass}>{launchAgentPrefix}</span>}
                        <span className={isSkin ? "truncate text-[10px] font-semibold leading-none" : "truncate text-[12px] font-semibold"}>
                          {launchAgentStripLabel}
                        </span>
                      </div>
                      {launchAgentSuffix && <span className={composerAddonClass}>{launchAgentSuffix}</span>}
                    </div>
                    <div
                      className={
                        isSkin
                          ? "vibe-scrollbar vibe-scrollbar-horizontal flex min-w-0 flex-1 gap-1 overflow-x-auto pb-0.5"
                          : "vibe-scrollbar vibe-scrollbar-horizontal flex gap-2 overflow-x-auto pb-1"
                      }
                    >
                      {agentOptions.map((option) => {
                        const active = createPlatform === option.platform;
                        return (
                          <button
                            aria-pressed={active}
                            className={agentOptionClass(active)}
                            key={option.platform}
                            onClick={() => {
                              setCreatePlatform(option.platform);
                              playSkinAudioEvent("agentSelect");
                            }}
                            type="button"
                          >
                            <AgentIcon className={isSkin ? "h-4 w-4" : "h-5 w-5"} platform={option.platform} />
                            <span>{option.label}</span>
                          </button>
                        );
                      })}
                    </div>
                  </div>

                  {(launchExtraLabel || launchExtraValue) && (
                    <div className={isSkin ? "mt-1 flex flex-wrap gap-1" : "mt-3 flex flex-wrap gap-2"}>
                      {launchExtraLabel && <span className={composerAddonClass}>{launchExtraLabel}</span>}
                      {launchExtraValue && <span className={composerAddonClass}>{launchExtraValue}</span>}
                    </div>
                  )}

                  {agentCatalogNotice}
                  {agentMissingNotice}

                  <div className={composerClass}>
                    <textarea
                      aria-label={launchPlaceholder}
                      className={composerInputClass}
                      onChange={(event) => setLaunchPrompt(event.target.value)}
                      placeholder={launchPlaceholder}
                      value={launchPrompt}
                    />
                    <div className={composerMetaBarClass}>
                      <label className={`${composerLabelClass} ${isSkin ? "sm:max-w-[12rem]" : "sm:max-w-[18rem]"}`}>
                        <span className={composerLabelTextClass}>{launchFolderLabel}</span>
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
                          {desktop && (
                            <option value={chooseFolderOptionValue}>{t("vibe.launchNewFolder")}</option>
                          )}
                        </select>
                      </label>
                      <label className={`${composerLabelClass} ${isSkin ? "sm:max-w-[8.25rem]" : "sm:max-w-[11rem]"}`}>
                        <span className={composerLabelTextClass}>{launchModelLabel}</span>
                        <select
                          className={composerControlClass}
                          disabled={launchModelChoices.length === 0}
                          onChange={(event) => setLaunchModel(event.target.value)}
                          value={launchModel}
                        >
                          <option value={autoLaunchOptionValue}>{t("vibe.launchAuto")}</option>
                          {launchModelChoices.map((model) => (
                            <option key={model.id} value={model.id}>
                              {model.id}
                            </option>
                          ))}
                        </select>
                      </label>
                      <label className={`${composerLabelClass} ${isSkin ? "sm:max-w-[8.25rem]" : "sm:max-w-[10rem]"}`}>
                        <span className={composerLabelTextClass}>{launchReasoningLabel}</span>
                        <select
                          className={composerControlClass}
                          disabled={launchReasoningChoices.length === 0}
                          onChange={(event) => setLaunchReasoning(event.target.value)}
                          value={launchReasoning}
                        >
                          <option value={autoLaunchOptionValue}>{t("vibe.launchAuto")}</option>
                          {launchReasoningChoices.map((level) => (
                            <option key={level.effort} value={level.effort}>
                              {level.effort}
                            </option>
                          ))}
                        </select>
                      </label>
                      <button
                        className={composerSendButtonClass}
                        disabled={!agentInstalled}
                        onClick={launchFromComposer}
                        type="button"
                      >
                        <SendHorizontal className={isSkin ? "h-3.5 w-3.5" : "h-4 w-4"} />
                        <span>{launchSendLabel}</span>
                      </button>
                    </div>
                  </div>
                </section>
              </div>
            )}
            {tiledTerminals && tabs.length > 0 ? (
              <div
                className={`vibe-scrollbar ${scrollbarThemeClass} vibe-scrollbar-active vibe-tiled-terminals flex h-full min-h-0 gap-2 overflow-x-auto overflow-y-hidden`}
                data-testid="vibe-tiled-terminals"
              >
                {tabs.map((tab) => (
                  <div
                    aria-current={tab.id === activeId ? "true" : undefined}
                    className={
                      isSkin
                        ? `vibe-tiled-terminal h-full min-h-0 shrink-0 overflow-hidden rounded-lg border border-[var(--vibe-border)] ${
                            tab.id === activeId ? "vibe-tiled-terminal-active" : ""
                          }`
                        : isDark
                          ? `vibe-tiled-terminal h-full min-h-0 shrink-0 overflow-hidden rounded-xl border border-[#073642] ${
                              tab.id === activeId ? "vibe-tiled-terminal-active" : ""
                            }`
                          : `vibe-tiled-terminal h-full min-h-0 shrink-0 overflow-hidden rounded-xl border border-stone-200 ${
                              tab.id === activeId ? "vibe-tiled-terminal-active" : ""
                            }`
                    }
                    key={tab.id}
                    onFocus={() => setActiveId(tab.id)}
                    ref={tab.id === activeId ? activeTileRef : undefined}
                  >
                    <XtermPane
                      active
                      onStatusChange={updateStatus}
                      session={tab}
                      themeMode={terminalThemeMode}
                      themeOverride={isSkin ? activeSkin.terminal : undefined}
                      transparentSurface={isSkin}
                    />
                  </div>
                ))}
              </div>
            ) : (
              tabs.map((tab) => (
                <XtermPane
                  active={tab.id === activeId}
                  key={tab.id}
                  onStatusChange={updateStatus}
                  session={tab}
                  themeMode={terminalThemeMode}
                  themeOverride={isSkin ? activeSkin.terminal : undefined}
                  transparentSurface={isSkin}
                />
              ))
            )}
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
                      "",
                      handleHologramInteract,
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
                  onHologramInteract={handleHologramInteract}
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
              <div className="absolute bottom-full left-2 z-40 mb-2" ref={startMenuRef}>
                <MotionMenu
                  ariaLabel="开始菜单"
                  className="vibe-skin-taskbar-start-menu w-64 overflow-hidden rounded-2xl border p-2"
                  open={startMenuOpen}
                  origin="bottom-left"
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
                        className="vibe-skin-taskbar-menu-item flex w-full items-center rounded-xl px-3 py-2 text-left text-[12px] motion-control disabled:cursor-not-allowed disabled:opacity-50"
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
                </MotionMenu>
              </div>
            )}

            <button
              aria-expanded={startMenuOpen}
              className="vibe-skin-taskbar-start-button inline-flex h-8 shrink-0 items-center gap-2 border px-4 text-[13px] font-bold italic tracking-wide motion-control"
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

      <AnimatePresence initial={false}>
        {appearanceOpen ? (
        <motion.div
          className="motion-runtime-overlay fixed inset-0 z-50 grid place-items-center bg-black/45 p-4"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={isJSDOM ? undefined : { opacity: 0 }}
          transition={{ duration: 0.24, ease: [0.22, 1, 0.36, 1] }}
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
                    ? "vibe-skin-ghost grid h-8 w-8 place-items-center rounded-xl border motion-control"
                    : isDark
                      ? "grid h-8 w-8 place-items-center rounded-xl border border-[#586e75] text-[#93a1a1] motion-control hover:text-[#fdf6e3]"
                      : "grid h-8 w-8 place-items-center rounded-xl border border-stone-200 text-stone-500 motion-control hover:text-stone-950"
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
                      ? "vibe-skin-select mt-1 w-full rounded-xl border px-3 py-2 text-[13px] outline-none motion-control"
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

              <label
                className={
                  isSkin
                    ? "vibe-skin-ghost flex items-center justify-between gap-3 rounded-2xl border px-3 py-2 text-[12px] font-semibold text-[var(--vibe-text)]"
                    : isDark
                      ? "flex items-center justify-between gap-3 rounded-2xl border border-[#586e75] px-3 py-2 text-[12px] font-semibold text-[#fdf6e3]"
                      : "flex items-center justify-between gap-3 rounded-2xl border border-stone-200 px-3 py-2 text-[12px] font-semibold text-stone-700"
                }
              >
                <span>{t("vibe.skinAudioEnabled")}</span>
                <input
                  checked={skinAudioEnabled}
                  className={isSkin ? "h-4 w-4 accent-[var(--vibe-accent)]" : "h-4 w-4 accent-blue-500"}
                  onChange={(event) => setSkinAudioEnabled(event.target.checked)}
                  type="checkbox"
                />
              </label>

              <div className="flex flex-wrap gap-2">
                <button
                  className={
                    isSkin
                      ? "vibe-skin-ghost inline-flex items-center gap-2 rounded-xl border px-3 py-2 text-[12px] font-semibold motion-control"
                      : isDark
                        ? "inline-flex items-center gap-2 rounded-xl border border-[#586e75] px-3 py-2 text-[12px] font-semibold text-[#93a1a1] motion-control hover:text-[#fdf6e3]"
                        : "inline-flex items-center gap-2 rounded-xl border border-stone-200 px-3 py-2 text-[12px] font-semibold text-stone-600 motion-control hover:text-stone-950"
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
                        ? "vibe-skin-danger rounded-xl border px-3 py-2 text-[12px] font-semibold motion-control"
                        : isDark
                          ? "rounded-xl border border-red-400/60 px-3 py-2 text-[12px] font-semibold text-red-200 motion-control hover:bg-red-500/20"
                          : "rounded-xl border border-red-200 px-3 py-2 text-[12px] font-semibold text-red-700 motion-control hover:bg-red-50"
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
        </motion.div>
        ) : null}
      </AnimatePresence>

      <AnimatePresence initial={false}>
        {tabSettingsOpen ? (
        <motion.div
          className="motion-runtime-overlay fixed inset-0 z-50 grid place-items-center bg-black/45 p-4"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.24, ease: [0.22, 1, 0.36, 1] }}
          onMouseDown={(event) => {
            if (event.target === event.currentTarget) {
              setTabSettingsOpen(false);
            }
          }}
        >
          <div
            aria-labelledby="vibe-tab-settings-title"
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
                <h2 id="vibe-tab-settings-title" className="text-base font-semibold">
                  {t("vibe.tabSettingsTitle")}
                </h2>
                <p className={isSkin ? "mt-1 text-[12px] text-[var(--vibe-muted-text)]" : isDark ? "mt-1 text-[12px] text-[#93a1a1]" : "mt-1 text-[12px] text-stone-500"}>
                  {t("vibe.tabSettingsSubtitle")}
                </p>
              </div>
              <button
                aria-label={t("vibe.cancel")}
                className={
                  isSkin
                    ? "vibe-skin-ghost grid h-8 w-8 place-items-center rounded-xl border motion-control"
                    : isDark
                      ? "grid h-8 w-8 place-items-center rounded-xl border border-[#586e75] text-[#93a1a1] motion-control hover:text-[#fdf6e3]"
                      : "grid h-8 w-8 place-items-center rounded-xl border border-stone-200 text-stone-500 motion-control hover:text-stone-950"
                }
                onClick={() => setTabSettingsOpen(false)}
                type="button"
              >
                <X className="h-4 w-4" />
              </button>
            </div>

            <div className="space-y-4">
              <label className={isSkin ? "block text-[12px] font-semibold text-[var(--vibe-muted-text)]" : isDark ? "block text-[12px] font-semibold text-[#93a1a1]" : "block text-[12px] font-semibold text-stone-600"}>
                <span className="flex items-center justify-between gap-3">
                  <span>{t("vibe.tileWidth")}</span>
                  <span className="tabular-nums">
                    {t("vibe.tileWidthValue", { width: String(tileWidth) })}
                  </span>
                </span>
                <input
                  aria-label={t("vibe.tileWidth")}
                  className={isSkin ? "mt-2 w-full accent-[var(--vibe-accent)]" : "mt-2 w-full accent-blue-500"}
                  max={TILE_MAX_WIDTH}
                  min={TILE_MIN_WIDTH}
                  onChange={(event) => setTileWidth(clampTileWidth(Number(event.target.value)))}
                  step={TILE_WIDTH_STEP}
                  type="range"
                  value={tileWidth}
                />
              </label>

              <label
                className={
                  isSkin
                    ? "vibe-skin-ghost flex items-center justify-between gap-3 rounded-2xl border px-3 py-2 text-[12px] font-semibold text-[var(--vibe-text)]"
                    : isDark
                      ? "flex items-center justify-between gap-3 rounded-2xl border border-[#586e75] px-3 py-2 text-[12px] font-semibold text-[#fdf6e3]"
                      : "flex items-center justify-between gap-3 rounded-2xl border border-stone-200 px-3 py-2 text-[12px] font-semibold text-stone-700"
                }
              >
                <span>{t("vibe.tabWidthFitsContent")}</span>
                <input
                  checked={tabWidthFitsContent}
                  className={isSkin ? "h-4 w-4 accent-[var(--vibe-accent)]" : "h-4 w-4 accent-blue-500"}
                  onChange={(event) => setTabWidthFitsContent(event.target.checked)}
                  type="checkbox"
                />
              </label>
              <p className={isSkin ? "text-[12px] leading-5 text-[var(--vibe-muted-text)]" : isDark ? "text-[12px] leading-5 text-[#93a1a1]" : "text-[12px] leading-5 text-stone-500"}>
                {t("vibe.tabWidthFitsContentHelp")}
              </p>
            </div>

            <div className="mt-5 flex justify-end">
              <button
                className={
                  isSkin
                    ? "vibe-skin-primary rounded-xl border px-3 py-2 text-[13px] font-semibold motion-control"
                    : isDark
                      ? "rounded-xl bg-[#268bd2] px-3 py-2 text-[13px] font-semibold text-[#002b36] motion-control hover:bg-[#2aa198]"
                      : "rounded-xl bg-stone-950 px-3 py-2 text-[13px] font-semibold text-white motion-control hover:bg-stone-800"
                }
                onClick={() => setTabSettingsOpen(false)}
                type="button"
              >
                {t("vibe.close")}
              </button>
            </div>
          </div>
        </motion.div>
        ) : null}
      </AnimatePresence>

      <AnimatePresence initial={false}>
        {createDialogOpen ? (
        <motion.div
          className="motion-runtime-overlay fixed inset-0 z-50 grid place-items-center bg-black/55 p-4"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.24, ease: [0.22, 1, 0.36, 1] }}
          onMouseDown={(event) => {
            if (event.target === event.currentTarget) {
              setCreateDialogOpen(false);
            }
          }}
        >
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
                    ? "vibe-skin-ghost grid h-8 w-8 place-items-center rounded-xl border motion-control"
                    : isDark
                    ? "grid h-8 w-8 place-items-center rounded-xl border border-[#586e75] text-[#93a1a1] motion-control hover:text-[#fdf6e3]"
                    : "grid h-8 w-8 place-items-center rounded-xl border border-stone-200 text-stone-500 motion-control hover:text-stone-950"
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
                      ? "vibe-skin-field mt-1 w-full rounded-xl border px-3 py-2 text-[13px] outline-none motion-control"
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

              {agentCatalogNotice}
              {agentMissingNotice}

              <label className={isSkin ? "block text-[12px] font-semibold text-[var(--vibe-muted-text)]" : isDark ? "block text-[12px] font-semibold text-[#93a1a1]" : "block text-[12px] font-semibold text-stone-600"}>
                {t("vibe.launchModel")}
                <select
                  className={
                    isSkin
                      ? "vibe-skin-field mt-1 w-full rounded-xl border px-3 py-2 text-[13px] outline-none motion-control"
                      : isDark
                      ? "mt-1 w-full rounded-xl border border-[#586e75] bg-[#073642] px-3 py-2 text-[13px] text-[#fdf6e3] outline-none focus:border-[#268bd2]"
                      : "mt-1 w-full rounded-xl border border-stone-200 bg-stone-50 px-3 py-2 text-[13px] text-stone-950 outline-none focus:border-blue-400"
                  }
                  disabled={launchModelChoices.length === 0}
                  onChange={(event) => setLaunchModel(event.target.value)}
                  value={launchModel}
                >
                  <option value={autoLaunchOptionValue}>{t("vibe.launchAuto")}</option>
                  {launchModelChoices.map((model) => (
                    <option key={model.id} value={model.id}>
                      {model.id}
                    </option>
                  ))}
                </select>
              </label>

              {launchReasoningChoices.length > 0 && (
                <label className={isSkin ? "block text-[12px] font-semibold text-[var(--vibe-muted-text)]" : isDark ? "block text-[12px] font-semibold text-[#93a1a1]" : "block text-[12px] font-semibold text-stone-600"}>
                  {t("vibe.launchReasoning")}
                  <select
                    className={
                      isSkin
                        ? "vibe-skin-field mt-1 w-full rounded-xl border px-3 py-2 text-[13px] outline-none motion-control"
                        : isDark
                        ? "mt-1 w-full rounded-xl border border-[#586e75] bg-[#073642] px-3 py-2 text-[13px] text-[#fdf6e3] outline-none focus:border-[#268bd2]"
                        : "mt-1 w-full rounded-xl border border-stone-200 bg-stone-50 px-3 py-2 text-[13px] text-stone-950 outline-none focus:border-blue-400"
                    }
                    onChange={(event) => setLaunchReasoning(event.target.value)}
                    value={launchReasoning}
                  >
                    <option value={autoLaunchOptionValue}>{t("vibe.launchAuto")}</option>
                    {launchReasoningChoices.map((level) => (
                      <option key={level.effort} value={level.effort}>
                        {level.effort}
                      </option>
                    ))}
                  </select>
                </label>
              )}

              {projectDirectories.length > 0 && (
                <label className={isSkin ? "block text-[12px] font-semibold text-[var(--vibe-muted-text)]" : isDark ? "block text-[12px] font-semibold text-[#93a1a1]" : "block text-[12px] font-semibold text-stone-600"}>
                  {t("vibe.existingFolder")}
                  <select
                    className={
                      isSkin
                        ? "vibe-skin-field mt-1 w-full rounded-xl border px-3 py-2 text-[13px] outline-none motion-control"
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
                        ? "vibe-skin-field min-w-0 flex-1 rounded-xl border px-3 py-2 text-[13px] outline-none motion-control"
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
                        ? "vibe-skin-ghost shrink-0 rounded-xl border px-3 py-2 text-[13px] font-semibold motion-control disabled:cursor-not-allowed disabled:opacity-50"
                        : isDark
                        ? "shrink-0 rounded-xl border border-[#586e75] bg-[#073642] px-3 py-2 text-[13px] font-semibold text-[#fdf6e3] motion-control hover:border-[#839496] disabled:cursor-not-allowed disabled:opacity-50"
                        : "shrink-0 rounded-xl border border-stone-200 bg-white px-3 py-2 text-[13px] font-semibold text-stone-700 motion-control hover:border-stone-300 disabled:cursor-not-allowed disabled:opacity-50"
                    }
                    disabled={!desktop}
                    onClick={() => void chooseFolder()}
                    title={desktop ? undefined : t("common.desktopOnly")}
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
                    ? "vibe-skin-ghost rounded-xl border px-3 py-2 text-[13px] font-semibold motion-control"
                    : isDark
                    ? "rounded-xl border border-[#586e75] px-3 py-2 text-[13px] font-semibold text-[#93a1a1] motion-control hover:text-[#fdf6e3]"
                    : "rounded-xl border border-stone-200 px-3 py-2 text-[13px] font-semibold text-stone-600 motion-control hover:text-stone-950"
                }
                onClick={() => setCreateDialogOpen(false)}
                type="button"
              >
                {t("vibe.cancel")}
              </button>
              <button
                className={
                  isSkin
                    ? "vibe-skin-primary rounded-xl border px-3 py-2 text-[13px] font-semibold motion-control"
                    : isDark
                    ? "rounded-xl bg-[#b58900] px-3 py-2 text-[13px] font-semibold text-[#002b36] motion-control hover:bg-[#cb4b16] hover:text-white"
                    : "rounded-xl bg-stone-950 px-3 py-2 text-[13px] font-semibold text-white motion-control hover:bg-stone-800"
                }
                disabled={!agentInstalled}
                onClick={launchNewAgent}
                type="button"
              >
                {t("vibe.create")}
              </button>
            </div>
          </div>
        </motion.div>
        ) : null}
      </AnimatePresence>
    </main>
  );
}
