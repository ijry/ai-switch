import { useQuery } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import {
  ChevronDown,
  ChevronRight,
  FolderOpen,
  MoonStar,
  PanelLeftClose,
  Play,
  Plus,
  SunMedium,
  TerminalSquare,
  X,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createTerminalSession, killTerminalSession, listSessions } from "../lib/api/client";
import { useI18n } from "../lib/i18n";
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

type VibeTheme = "dark" | "light";

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

export function VibeScreen({ onExitVibe }: VibeScreenProps) {
  const { t } = useI18n();
  const [tabs, setTabs] = useState<TerminalSession[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [createDialogOpen, setCreateDialogOpen] = useState(false);
  const [createProjectDir, setCreateProjectDir] = useState("");
  const [createPlatform, setCreatePlatform] = useState<(typeof agentOptions)[number]["platform"]>("codex");
  const [themeMode, setThemeMode] = useState<VibeTheme>("dark");
  const [error, setError] = useState<string | null>(null);
  const [sessionListScrolling, setSessionListScrolling] = useState(false);
  const [expandedDirectories, setExpandedDirectories] = useState<Set<string>>(() => new Set());
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

  return (
    <main className={isDark ? "h-screen max-h-[100dvh] overflow-hidden bg-[#002b36] text-[#d8e2dc]" : "h-screen max-h-[100dvh] overflow-hidden text-stone-950"}>
      <div
        className={
          isDark
            ? "grid h-full min-h-0 grid-cols-1 lg:grid-cols-[356px_minmax(0,1fr)]"
            : "grid h-full min-h-0 grid-cols-1 lg:grid-cols-[356px_minmax(0,1fr)]"
        }
      >
        <aside
          className={
            isDark
              ? "relative flex h-full min-h-0 flex-col overflow-hidden border-b border-[#073642] bg-[#002b36] p-3 shadow-2xl shadow-black/25 lg:border-b-0 lg:border-r lg:border-[#073642]"
              : "relative flex h-full min-h-0 flex-col overflow-hidden border-b border-white/70 bg-gradient-to-br from-slate-50/92 via-emerald-50/74 to-amber-50/70 p-3 shadow-xl shadow-stone-900/5 backdrop-blur-2xl lg:border-b-0 lg:border-r lg:border-white/80"
          }
        >
          <div
            className={
              isDark
                ? "pointer-events-none absolute inset-0 bg-[radial-gradient(circle_at_18%_12%,rgba(38,139,210,0.18),transparent_30%),radial-gradient(circle_at_90%_10%,rgba(181,137,0,0.18),transparent_28%),linear-gradient(180deg,rgba(7,54,66,0.78),rgba(0,43,54,0.92))]"
                : "pointer-events-none absolute inset-0 bg-[radial-gradient(circle_at_20%_15%,rgba(16,185,129,0.18),transparent_34%),radial-gradient(circle_at_88%_8%,rgba(245,158,11,0.16),transparent_30%),linear-gradient(180deg,rgba(255,255,255,0.72),rgba(255,255,255,0.38))]"
            }
          />
          <div className="relative flex min-h-0 flex-1 flex-col">
            <div
              className={
                isDark
                  ? "mb-4 flex items-start justify-between gap-3 rounded-2xl border border-[#073642] bg-[#073642]/65 p-3 shadow-sm backdrop-blur-xl"
                  : "mb-4 flex items-start justify-between gap-3 rounded-2xl border border-white/80 bg-white/56 p-3 shadow-sm backdrop-blur-xl"
              }
            >
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
            </div>

            <div
              className={
                isDark
                  ? "mb-2 flex items-center gap-2 rounded-2xl border border-[#073642] bg-[#073642]/55 p-3"
                  : "mb-2 flex items-center gap-2 rounded-2xl border border-white/80 bg-white/56 p-3 shadow-sm backdrop-blur-xl"
              }
            >
              <button
                className={
                  isDark
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
                  isDark
                    ? "inline-flex h-9 shrink-0 items-center gap-1.5 rounded-xl border border-[#586e75] bg-[#002b36] px-2 text-[12px] font-semibold text-[#fdf6e3] transition-colors hover:border-[#839496] hover:bg-[#073642]"
                    : "inline-flex h-9 shrink-0 items-center gap-1.5 rounded-xl border border-stone-200 bg-white px-2 text-[12px] font-semibold text-stone-700 transition-colors hover:border-stone-300 hover:bg-stone-50"
                }
                onClick={() => setThemeMode((current) => (current === "dark" ? "light" : "dark"))}
                type="button"
              >
                {isDark ? <SunMedium className="h-4 w-4" /> : <MoonStar className="h-4 w-4" />}
                <span>{isDark ? t("vibe.themeDark") : t("vibe.themeLight")}</span>
              </button>

              {error && (
                <p className="absolute left-3 right-3 top-[8.75rem] rounded-xl border border-red-400/40 bg-red-950/90 p-2 text-[12px] text-red-100 shadow-lg">
                  {error}
                </p>
              )}
            </div>

            <div
              className={`vibe-scrollbar ${isDark ? "vibe-scrollbar-dark" : "vibe-scrollbar-light"} ${
                sessionListScrolling ? "vibe-scrollbar-active" : ""
              } min-h-0 flex-1 space-y-3 overflow-y-auto p-3`}
              onScroll={markSessionListScrolling}
            >
              {sessionsQuery.isLoading && (
                <p className={isDark ? "text-sm text-[#93a1a1]" : "text-sm text-zinc-400"}>
                  {t("vibe.loadingSessions")}
                </p>
              )}
              {!sessionsQuery.isLoading && groupedSessions.length === 0 && (
                <p
                  className={
                    isDark
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
                      isDark
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
                        isDark
                          ? "flex w-full items-center gap-2 rounded-xl px-1 py-1 text-left text-[12px] font-semibold text-[#fdf6e3] transition hover:bg-[#002b36]/55"
                          : "flex w-full items-center gap-2 rounded-xl px-1 py-1 text-left text-[12px] font-semibold text-stone-800 transition hover:bg-stone-100/80"
                      }
                      onClick={() => toggleDirectory(group.directory)}
                      type="button"
                    >
                      <ToggleIcon className={isDark ? "h-3.5 w-3.5 shrink-0 text-[#93a1a1]" : "h-3.5 w-3.5 shrink-0 text-stone-400"} />
                      <FolderOpen className={isDark ? "h-4 w-4 shrink-0 text-[#b58900]" : "h-4 w-4 shrink-0 text-amber-600"} />
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
                                isDark
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
                                <Play className={isDark ? "h-3.5 w-3.5 shrink-0 text-[#b58900]" : "h-3.5 w-3.5 shrink-0 text-amber-600"} />
                              </span>
                              <span className={isDark ? "mt-0.5 block truncate text-[11px] text-[#93a1a1]" : "mt-0.5 block truncate text-[11px] text-stone-500"}>
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
            isDark
              ? "flex h-full min-h-0 min-w-0 flex-col overflow-hidden bg-[#002b36] shadow-xl shadow-black/20"
              : "flex h-full min-h-0 min-w-0 flex-col overflow-hidden bg-slate-50 shadow-xl shadow-stone-900/5"
          }
        >
          <div
            className={
              isDark
                ? "vibe-scrollbar vibe-scrollbar-dark vibe-scrollbar-horizontal flex h-10 shrink-0 items-stretch gap-0 overflow-x-auto border-b border-[#073642] bg-[#00212b] px-1"
                : "vibe-scrollbar vibe-scrollbar-light vibe-scrollbar-horizontal flex h-10 shrink-0 items-stretch gap-0 overflow-x-auto border-b border-stone-200 bg-white/85 px-1"
            }
          >
            {tabs.length === 0 && (
              <p className={isDark ? "flex items-center px-3 text-[12px] text-[#93a1a1]" : "flex items-center px-3 text-[12px] text-stone-500"}>
                {t("vibe.noTabs")}
              </p>
            )}
            {tabs.map((tab) => (
              <div
                className={`group relative inline-flex h-full max-w-[220px] min-w-[132px] shrink-0 items-center border-r ${
                  activeId === tab.id
                    ? isDark
                      ? "border-[#073642] bg-[#002b36] text-[#fdf6e3]"
                      : "border-stone-200 bg-slate-50 text-stone-950"
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
                      isDark
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
                    isDark
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

          <div className="min-h-0 flex-1 overflow-hidden">
            {!activeTab && (
              <div
                className={
                  isDark
                    ? "grid h-full place-items-center text-center"
                    : "grid h-full place-items-center text-center"
                }
              >
                <div>
                  <TerminalSquare className={isDark ? "mx-auto h-8 w-8 text-[#586e75]" : "mx-auto h-8 w-8 text-stone-400"} />
                  <p className={isDark ? "mt-2 text-sm font-semibold text-[#fdf6e3]" : "mt-2 text-sm font-semibold text-stone-900"}>
                    {t("vibe.emptyTitle")}
                  </p>
                  <p className={isDark ? "mt-1 text-[13px] text-[#93a1a1]" : "mt-1 text-[13px] text-stone-500"}>
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
                themeMode={themeMode}
              />
            ))}
          </div>
        </div>
      </div>

      {createDialogOpen && (
        <div className="fixed inset-0 z-50 grid place-items-center bg-black/55 p-4">
          <div
            className={
              isDark
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
                <p className={isDark ? "mt-1 text-[12px] text-[#93a1a1]" : "mt-1 text-[12px] text-stone-500"}>
                  {t("vibe.createSubtitle")}
                </p>
              </div>
              <button
                aria-label={t("vibe.cancel")}
                className={
                  isDark
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
              <label className={isDark ? "block text-[12px] font-semibold text-[#93a1a1]" : "block text-[12px] font-semibold text-stone-600"}>
                {t("vibe.agent")}
                <select
                  className={
                    isDark
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
                <label className={isDark ? "block text-[12px] font-semibold text-[#93a1a1]" : "block text-[12px] font-semibold text-stone-600"}>
                  {t("vibe.existingFolder")}
                  <select
                    className={
                      isDark
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

              <label className={isDark ? "block text-[12px] font-semibold text-[#93a1a1]" : "block text-[12px] font-semibold text-stone-600"}>
                {t("vibe.projectDirectory")}
                <div className="mt-1 flex gap-2">
                  <input
                    className={
                      isDark
                        ? "min-w-0 flex-1 rounded-xl border border-[#586e75] bg-[#073642] px-3 py-2 text-[13px] text-[#fdf6e3] outline-none placeholder:text-[#586e75] focus:border-[#268bd2]"
                        : "min-w-0 flex-1 rounded-xl border border-stone-200 bg-stone-50 px-3 py-2 text-[13px] text-stone-950 outline-none focus:border-blue-400"
                    }
                    onChange={(event) => setCreateProjectDir(event.target.value)}
                    placeholder={t("vibe.projectPlaceholder")}
                    value={createProjectDir}
                  />
                  <button
                    className={
                      isDark
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
                  isDark
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
                  isDark
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
