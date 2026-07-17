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
  SunMedium,
  TerminalSquare,
  Upload,
  X,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { ChangeEvent } from "react";
import { createTerminalSession, killTerminalSession, listSessions } from "../lib/api/client";
import { useI18n } from "../lib/i18n";
import {
  BUILT_IN_VIBE_SKINS,
  clearStoredVibeSkin,
  importVibeSkinPackage,
  VIBE_SKIN_REGION_KEYS,
  readStoredVibeSkin,
  skinToCssVariables,
  writeStoredVibeSkin,
} from "../lib/vibeSkin";
import type { VibeSkinDefinition } from "../lib/vibeSkin";
import { AiSwitchLogo } from "../components/brand/AiSwitchLogo";
import type {
  CreateTerminalSessionInput,
  SessionMeta,
  TerminalSession,
  TerminalStatus,
} from "../lib/api/types";
import { XtermPane } from "../components/terminal/XtermPane";

const agentOptions = [
  { platform: "codex", label: "Codex" },
  { platform: "claude", label: "Claude" },
  { platform: "gemini", label: "Gemini" },
  { platform: "opencode", label: "OpenCode" },
  { platform: "openclaw", label: "OpenClaw" },
  { platform: "hermes", label: "Hermes" },
] as const;

type VibeTheme = "dark" | "light" | "skin";

type VibeScreenProps = {
  onExitVibe?: () => void;
};

function titleForSession(session: SessionMeta) {
  return session.title?.trim() || session.projectDir?.trim() || session.sessionId;
}

function directoryLabel(session: SessionMeta, unknownLabel: string) {
  return session.projectDir?.trim() || unknownLabel;
}

function groupSessions(sessions: SessionMeta[], unknownLabel: string) {
  const groups = new Map<string, SessionMeta[]>();
  for (const session of sessions) {
    const directory = directoryLabel(session, unknownLabel);
    groups.set(directory, [...(groups.get(directory) ?? []), session]);
  }
  return Array.from(groups.entries()).map(([directory, items]) => ({
    directory,
    items,
  }));
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

function statusDotClass(status: TerminalStatus, isActive: boolean, isDark: boolean) {
  if (status === "running") {
    return isActive ? "bg-emerald-500" : isDark ? "bg-[#859900]" : "bg-emerald-400";
  }
  if (status === "error") {
    return isActive ? "bg-red-500" : isDark ? "bg-[#dc322f]" : "bg-red-400";
  }
  return isActive ? "bg-stone-400" : isDark ? "bg-[#586e75]" : "bg-stone-500";
}

function statusLabel(status: TerminalStatus, t: (key: "vibe.status.running" | "vibe.status.exited" | "vibe.status.error") => string) {
  return t(`vibe.status.${status}` as "vibe.status.running" | "vibe.status.exited" | "vibe.status.error");
}

function nextVibeTheme(current: VibeTheme): VibeTheme {
  if (current === "dark") {
    return "light";
  }
  if (current === "light") {
    return "skin";
  }
  return "dark";
}

export function VibeScreen({ onExitVibe }: VibeScreenProps) {
  const { t } = useI18n();
  const [tabs, setTabs] = useState<TerminalSession[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [createDialogOpen, setCreateDialogOpen] = useState(false);
  const [createProjectDir, setCreateProjectDir] = useState("");
  const [createPlatform, setCreatePlatform] = useState<(typeof agentOptions)[number]["platform"]>("codex");
  const [themeMode, setThemeMode] = useState<VibeTheme>("dark");
  const [customSkin, setCustomSkin] = useState<VibeSkinDefinition | null>(() => readStoredVibeSkin());
  const [activeSkinId, setActiveSkinId] = useState<string>(
    () => readStoredVibeSkin()?.id ?? BUILT_IN_VIBE_SKINS[0].id,
  );
  const [error, setError] = useState<string | null>(null);
  const [sessionListScrolling, setSessionListScrolling] = useState(false);
  const [expandedDirectories, setExpandedDirectories] = useState<Set<string>>(() => new Set());
  const skinFileInputRef = useRef<HTMLInputElement | null>(null);
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
      title: titleForSession(session),
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

  const launchNewAgent = () => {
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
    setCreateDialogOpen(false);
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
  const skinShowcase = activeSkin.showcase;
  const showSkinShowcase = Boolean(isSkin && skinShowcase && skinShowcase.enabled !== false);
  const skinBodyGridClass = showSkinShowcase
    ? "vibe-skin-body grid min-h-0 flex-1 grid-cols-1 lg:grid-cols-[300px_minmax(0,1fr)_236px]"
    : "vibe-skin-body grid min-h-0 flex-1 grid-cols-1 lg:grid-cols-[300px_minmax(0,1fr)]";
  const activeSkinRegionKeys = isSkin
    ? VIBE_SKIN_REGION_KEYS.filter((region) => Boolean(activeSkin.regions?.[region]))
    : [];

  return (
    <main
      className={
        isSkin
          ? "vibe-skin h-screen max-h-[100dvh] overflow-hidden text-[var(--vibe-text)]"
          : isDark
            ? "h-screen max-h-[100dvh] overflow-hidden bg-[#002b36] text-[#d8e2dc]"
            : "h-screen max-h-[100dvh] overflow-hidden text-stone-950"
      }
      style={skinStyle}
    >
      <div className={isSkin ? "vibe-skin-frame flex h-full min-h-0 flex-col" : "grid h-full min-h-0 grid-cols-1 lg:grid-cols-[356px_minmax(0,1fr)]"}>
        {isSkin && (
          <div className="vibe-skin-titlebar flex h-10 shrink-0 items-center justify-between gap-3 border-b px-4 text-[11px] font-semibold uppercase tracking-[0.22em]">
            <div className="flex min-w-0 items-center gap-3">
              <span className="inline-flex h-4 w-4 shrink-0 rounded-full border border-[rgba(255,255,255,0.55)] bg-[var(--vibe-accent)] shadow-[inset_0_1px_0_rgba(255,255,255,0.58)]" />
              <div className="min-w-0">
                <p className="truncate">{t("vibe.title")}</p>
                <p className="truncate text-[10px] tracking-[0.18em] opacity-80">{activeSkin.name}</p>
              </div>
            </div>
            <div className="flex items-center gap-2 text-[10px] tracking-[0.18em] opacity-90">
              <span className="rounded-full border border-[rgba(255,255,255,0.45)] px-2 py-1">{themeLabel}</span>
              <span className="rounded-full border border-[rgba(255,255,255,0.45)] px-2 py-1">{activeSkin.author ?? "AI Switch"}</span>
            </div>
          </div>
        )}
        <div className={isSkin ? skinBodyGridClass : "contents"}>
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
              <div className="flex min-w-0 items-center gap-2">
                <AiSwitchLogo className="h-9 w-9 shrink-0 rounded-2xl shadow-sm" />
                <div className="min-w-0">
                  <h1 className={isSkin ? "truncate text-[13px] font-semibold text-[var(--vibe-text)]" : isDark ? "truncate text-[13px] font-semibold text-[#fdf6e3]" : "truncate text-[13px] font-semibold text-stone-950"}>
                    {t("vibe.title")} · {t("vibe.kicker")}
                  </h1>
                  <p className={isSkin ? "truncate text-[11px] text-[var(--vibe-muted-text)]" : isDark ? "truncate text-[11px] text-[#93a1a1]" : "truncate text-[11px] text-stone-500"}>
                    {t("vibe.subtitle")}
                  </p>
                </div>
              </div>
              <button
                aria-label={t("layout.switchToAgent")}
                className={
                  isSkin
                    ? "vibe-skin-ghost grid h-8 w-8 shrink-0 place-items-center rounded-xl border shadow-sm transition-colors focus:outline-none focus-visible:ring-2"
                    : isDark
                    ? "grid h-8 w-8 shrink-0 place-items-center rounded-xl border border-[#586e75] bg-[#073642] text-[#93a1a1] shadow-sm transition-colors hover:border-[#839496] hover:text-[#fdf6e3] focus:outline-none focus-visible:ring-2 focus-visible:ring-[#268bd2]"
                    : "grid h-8 w-8 shrink-0 place-items-center rounded-xl border border-stone-200 bg-white/70 text-stone-600 shadow-sm transition-colors hover:border-stone-300 hover:bg-white hover:text-stone-950 focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-400"
                }
                onClick={onExitVibe}
                type="button"
              >
                <PanelLeftClose className="h-4 w-4" />
              </button>
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
                onClick={() => setThemeMode((current) => nextVibeTheme(current))}
                type="button"
              >
                {isDark ? (
                  <SunMedium className="h-4 w-4" />
                ) : isSkin ? (
                  <Palette className="h-4 w-4" />
                ) : (
                  <MoonStar className="h-4 w-4" />
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
              <button
                aria-label={t("vibe.importSkin")}
                className={
                  isSkin
                    ? "vibe-skin-ghost inline-flex h-9 shrink-0 items-center gap-1.5 rounded-xl border px-2 text-[12px] font-semibold transition-colors"
                    : isDark
                      ? "inline-flex h-9 shrink-0 items-center gap-1.5 rounded-xl border border-[#586e75] bg-[#002b36] px-2 text-[12px] font-semibold text-[#fdf6e3] transition-colors hover:border-[#839496] hover:bg-[#073642]"
                      : "inline-flex h-9 shrink-0 items-center gap-1.5 rounded-xl border border-stone-200 bg-white px-2 text-[12px] font-semibold text-stone-700 transition-colors hover:border-stone-300 hover:bg-stone-50"
                }
                onClick={() => skinFileInputRef.current?.click()}
                type="button"
              >
                <Upload className="h-4 w-4" />
                <span>{t("vibe.importSkinShort")}</span>
              </button>

              {isSkin && (
                <div className="flex w-full items-center gap-2">
                  <select
                    aria-label={t("vibe.skinSelect")}
                    className="vibe-skin-select min-w-0 flex-1 rounded-xl border px-3 py-2 text-[12px] font-semibold outline-none transition"
                    onChange={(event) => setActiveSkinId(event.target.value)}
                    value={activeSkin.id}
                  >
                    {availableSkins.map((skin) => (
                      <option key={skin.id} value={skin.id}>
                        {skin.name}
                      </option>
                    ))}
                  </select>
                  {customSkin && (
                    <button
                      aria-label={t("vibe.clearSkin")}
                      className="vibe-skin-ghost inline-flex h-9 shrink-0 items-center gap-1.5 rounded-xl border px-2 text-[12px] font-semibold transition-colors"
                      onClick={clearCustomSkin}
                      type="button"
                    >
                      <X className="h-4 w-4" />
                      <span>{t("vibe.clearSkinShort")}</span>
                    </button>
                  )}
                </div>
              )}
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
              } ${isSkin ? "vibe-skin-session-list" : ""} min-h-0 flex-1 space-y-3 overflow-y-auto p-3`}
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
                const expanded = expandedDirectories.has(group.directory);
                const ToggleIcon = expanded ? ChevronDown : ChevronRight;
                return (
                  <div
                    className={
                      isSkin
                        ? "vibe-skin-group-panel rounded-2xl border p-2"
                        : isDark
                        ? "rounded-2xl border border-[#073642] bg-[#073642]/55 p-2"
                        : "rounded-2xl border border-stone-200 bg-white/70 p-2 shadow-sm"
                    }
                    key={group.directory}
                  >
                    <button
                      aria-expanded={expanded}
                      aria-label={
                        expanded
                          ? t("vibe.collapseDirectoryAria", { directory: group.directory })
                          : t("vibe.expandDirectoryAria", { directory: group.directory })
                      }
                      className={
                        isSkin
                          ? "vibe-skin-list-trigger flex w-full items-center gap-2 rounded-xl px-1 py-1 text-left text-[12px] font-semibold transition"
                          : isDark
                          ? "flex w-full items-center gap-2 rounded-xl px-1 py-1 text-left text-[12px] font-semibold text-[#fdf6e3] transition hover:bg-[#002b36]/55"
                          : "flex w-full items-center gap-2 rounded-xl px-1 py-1 text-left text-[12px] font-semibold text-stone-800 transition hover:bg-stone-100/80"
                      }
                      onClick={() => toggleDirectory(group.directory)}
                      type="button"
                    >
                      <ToggleIcon className={isSkin ? "h-3.5 w-3.5 shrink-0 text-[var(--vibe-muted-text)]" : isDark ? "h-3.5 w-3.5 shrink-0 text-[#93a1a1]" : "h-3.5 w-3.5 shrink-0 text-stone-400"} />
                      <FolderOpen className={isSkin ? "h-4 w-4 shrink-0 text-[var(--vibe-accent)]" : isDark ? "h-4 w-4 shrink-0 text-[#b58900]" : "h-4 w-4 shrink-0 text-amber-600"} />
                      <span className="truncate">{group.directory}</span>
                    </button>
                    {expanded && (
                      <div className="mt-2 space-y-1.5">
                        {group.items.map((session) => {
                          const canResume = Boolean(session.projectDir && session.resumeCommand);
                          const title = titleForSession(session);
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
                                  ? "w-full rounded-xl border border-[#073642] bg-[#002b36]/70 px-3 py-2 text-left text-[13px] text-[#fdf6e3] transition hover:border-[#b58900]/70 disabled:cursor-not-allowed disabled:opacity-45"
                                  : "w-full rounded-xl border border-stone-200 bg-stone-50/80 px-3 py-2 text-left text-[13px] text-stone-800 transition hover:border-amber-500/50 hover:bg-white disabled:cursor-not-allowed disabled:opacity-45"
                              }
                              disabled={!canResume}
                              key={sessionKey(session)}
                              onClick={() => resumeSession(session)}
                              type="button"
                            >
                              <span className="flex items-center justify-between gap-2">
                                <span className="truncate font-semibold">{title}</span>
                                <Play className={isSkin ? "h-3.5 w-3.5 shrink-0 text-[var(--vibe-accent)]" : isDark ? "h-3.5 w-3.5 shrink-0 text-[#b58900]" : "h-3.5 w-3.5 shrink-0 text-amber-600"} />
                              </span>
                              <span className={isSkin ? "mt-0.5 block truncate text-[11px] text-[var(--vibe-muted-text)]" : isDark ? "mt-0.5 block truncate text-[11px] text-[#93a1a1]" : "mt-0.5 block truncate text-[11px] text-stone-500"}>
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
                ? "vibe-scrollbar vibe-scrollbar-dark vibe-scrollbar-horizontal flex h-10 shrink-0 items-stretch gap-0 overflow-x-auto border-b border-[#073642] bg-[#00212b] px-1"
                : "vibe-scrollbar vibe-scrollbar-light vibe-scrollbar-horizontal flex h-10 shrink-0 items-stretch gap-0 overflow-x-auto border-b border-stone-200 bg-white/85 px-1"
            }
          >
            {tabs.length === 0 && (
              <p className={isSkin ? "flex items-center px-3 text-[12px] text-[var(--vibe-muted-text)]" : isDark ? "flex items-center px-3 text-[12px] text-[#93a1a1]" : "flex items-center px-3 text-[12px] text-stone-500"}>
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
                      ? "border-[#073642] bg-[#002b36] text-[#fdf6e3]"
                      : "border-stone-200 bg-slate-50 text-stone-950"
                    : isSkin
                      ? "vibe-skin-tab text-[var(--vibe-muted-text)]"
                      : isDark
                      ? "border-[#073642] bg-transparent text-[#93a1a1] hover:bg-[#073642]/55 hover:text-[#eee8d5]"
                      : "border-stone-200 bg-transparent text-stone-500 hover:bg-stone-100/80 hover:text-stone-900"
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
                        ? "absolute inset-x-0 bottom-0 h-[2px] bg-[#b58900]"
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
                      ? "vibe-skin-tab-close mr-1.5 grid h-5 w-5 shrink-0 place-items-center rounded-md opacity-70 transition group-hover:opacity-100"
                      : isDark
                      ? "mr-1.5 grid h-5 w-5 shrink-0 place-items-center rounded-md text-[#93a1a1] opacity-70 transition hover:bg-[#073642] hover:text-[#fdf6e3] group-hover:opacity-100"
                      : "mr-1.5 grid h-5 w-5 shrink-0 place-items-center rounded-md text-stone-400 opacity-70 transition hover:bg-stone-200 hover:text-stone-700 group-hover:opacity-100"
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
                    ? "vibe-skin-empty-state grid h-full place-items-center text-center"
                    : isDark
                    ? "grid h-full place-items-center text-center"
                    : "grid h-full place-items-center text-center"
                }
              >
                <div>
                  <TerminalSquare className={isSkin ? "mx-auto h-8 w-8 text-[var(--vibe-accent)]" : isDark ? "mx-auto h-8 w-8 text-[#586e75]" : "mx-auto h-8 w-8 text-stone-400"} />
                  <p className={isSkin ? "mt-2 text-sm font-semibold text-[var(--vibe-text)]" : isDark ? "mt-2 text-sm font-semibold text-[#fdf6e3]" : "mt-2 text-sm font-semibold text-stone-900"}>
                    {t("vibe.emptyTitle")}
                  </p>
                  <p className={isSkin ? "mt-1 text-[13px] text-[var(--vibe-muted-text)]" : isDark ? "mt-1 text-[13px] text-[#93a1a1]" : "mt-1 text-[13px] text-stone-500"}>
                    {t("vibe.emptyBody")}
                  </p>
                </div>
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
              />
            ))}
          </div>
        </div>

        {showSkinShowcase && (
          <aside className="vibe-skin-right-rail hidden min-h-0 flex-col overflow-hidden border-l p-3 lg:flex">
            <div className="vibe-skin-right-card flex min-h-0 flex-1 flex-col rounded-3xl border p-4">
              <div className="flex items-start justify-between gap-3">
                <div className="min-w-0">
                  <p className="text-[10px] font-semibold uppercase tracking-[0.24em] text-[var(--vibe-muted-text)]">
                    {skinShowcase?.badge ?? t("vibe.themeSkin")}
                  </p>
                  <h2 className="mt-1 truncate text-lg font-semibold text-[var(--vibe-text)]">
                    {skinShowcase?.title ?? activeSkin.name}
                  </h2>
                  <p className="mt-1 text-[12px] text-[var(--vibe-muted-text)]">
                    {skinShowcase?.subtitle ?? activeSkin.author ?? t("vibe.subtitle")}
                  </p>
                </div>
                {skinShowcase?.image ? (
                  <img
                    alt=""
                    className="h-20 w-20 shrink-0 rounded-2xl border object-cover"
                    src={skinShowcase.image}
                  />
                ) : (
                  <div className="vibe-skin-showcase-orb grid h-20 w-20 shrink-0 place-items-center rounded-2xl border">
                    <AiSwitchLogo className="h-10 w-10 rounded-xl" />
                  </div>
                )}
              </div>
              <p className="mt-4 text-[13px] leading-6 text-[var(--vibe-text)] opacity-90">
                {skinShowcase?.body ?? t("vibe.emptyBody")}
              </p>
              <div className="mt-auto pt-4">
                <div className="rounded-2xl border px-3 py-2 text-[11px] text-[var(--vibe-muted-text)]">
                  {skinShowcase?.footer ?? activeSkin.id}
                </div>
              </div>
            </div>
            <div className="vibe-skin-right-card mt-3 rounded-2xl border p-3">
              <p className="text-[10px] font-semibold uppercase tracking-[0.24em] text-[var(--vibe-muted-text)]">
                Regions
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
          </aside>
        )}
        </div>

        {isSkin && (
          <div className="vibe-skin-status-bar flex h-9 shrink-0 items-center justify-between gap-3 border-t px-4 text-[11px] font-medium">
            <span className="truncate">{activeSkin.name}</span>
            <span className="flex items-center gap-3">
              <span>{activeSkin.author ?? "AI Switch"}</span>
              <span>{tabs.length}</span>
              <span>{themeLabel}</span>
            </span>
          </div>
        )}
      </div>

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
                        {directory}
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
