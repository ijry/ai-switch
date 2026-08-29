import type { CreateTerminalSessionInput, TerminalLaunchKind } from "./api/types";

export const VIBE_TABS_STORAGE_KEY = "ai-switch.vibe.tabs";

// Restoring dozens of PTYs at startup would stall the app, so only the most
// recent tabs come back.
export const VIBE_TABS_RESTORE_LIMIT = 8;

export type VibeTabDescriptor = {
  input: CreateTerminalSessionInput;
  active?: boolean;
};

const launchKinds: TerminalLaunchKind[] = ["shell", "agent", "resume"];

function optionalText(value: unknown) {
  if (typeof value !== "string") {
    return undefined;
  }
  const trimmed = value.trim();
  return trimmed ? trimmed : undefined;
}

function optionalSize(value: unknown) {
  return typeof value === "number" && Number.isFinite(value) && value > 0
    ? Math.round(value)
    : undefined;
}

function parseDescriptor(raw: unknown): VibeTabDescriptor | null {
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) {
    return null;
  }
  const source = raw as Record<string, unknown>;
  const inputSource = source.input;
  if (!inputSource || typeof inputSource !== "object" || Array.isArray(inputSource)) {
    return null;
  }

  const candidate = inputSource as Record<string, unknown>;
  const kind = candidate.kind;
  const cwd = optionalText(candidate.cwd);
  if (typeof kind !== "string" || !launchKinds.includes(kind as TerminalLaunchKind) || !cwd) {
    return null;
  }

  const command = optionalText(candidate.command);
  // A resume tab is only reproducible when we still know the command to replay.
  if (kind === "resume" && !command) {
    return null;
  }

  return {
    input: {
      kind: kind as TerminalLaunchKind,
      platform: optionalText(candidate.platform) ?? null,
      command: command ?? null,
      title: optionalText(candidate.title) ?? null,
      cwd,
      cols: optionalSize(candidate.cols) ?? null,
      rows: optionalSize(candidate.rows) ?? null,
      model: optionalText(candidate.model) ?? null,
      reasoningEffort: optionalText(candidate.reasoningEffort) ?? null,
    },
    active: source.active === true,
  };
}

export function readStoredVibeTabs(): VibeTabDescriptor[] {
  try {
    const stored = window.localStorage.getItem(VIBE_TABS_STORAGE_KEY);
    if (!stored) {
      return [];
    }
    const raw = JSON.parse(stored) as unknown;
    if (!Array.isArray(raw)) {
      return [];
    }
    return raw
      .map(parseDescriptor)
      .filter((descriptor): descriptor is VibeTabDescriptor => descriptor !== null)
      .slice(0, VIBE_TABS_RESTORE_LIMIT);
  } catch {
    return [];
  }
}

export function writeStoredVibeTabs(descriptors: VibeTabDescriptor[]) {
  try {
    if (descriptors.length === 0) {
      window.localStorage.removeItem(VIBE_TABS_STORAGE_KEY);
      return;
    }
    window.localStorage.setItem(
      VIBE_TABS_STORAGE_KEY,
      JSON.stringify(descriptors.slice(0, VIBE_TABS_RESTORE_LIMIT)),
    );
  } catch {
    // Storage can be unavailable (private mode, quota); losing tab restore is
    // preferable to breaking the screen.
  }
}
