import { useQuery } from "@tanstack/react-query";
import {
  ArrowLeft,
  Check,
  ChevronDown,
  ChevronRight,
  Clock,
  Copy,
  FileText,
  FolderOpen,
  Hash,
  Layers3,
  ListTree,
  MoreHorizontal,
  MessageSquareText,
  Play,
  Rows3,
  Search,
  Terminal,
} from "lucide-react";
import { useDeferredValue, useEffect, useMemo, useRef, useState } from "react";
import { agentPlatforms, type AgentPlatform } from "../components/layout/AppLayout";
import { getSessionMessages, listSessions, openSessionTerminal } from "../lib/api/client";
import { useI18n, type Language } from "../lib/i18n";
import type { SessionMessage, SessionMeta } from "../lib/api/types";
import { isDesktop } from "../lib/transport";

type SessionsScreenProps = {
  initialPlatform?: string | null;
};

type ListMode = "flat" | "grouped";
type Translator = ReturnType<typeof useI18n>["t"];

const platformLabels: Record<AgentPlatform, string> = {
  codex: "Codex",
  claude: "Claude",
  grok: "Grok",
  gemini: "Gemini",
  opencode: "OpenCode",
  openclaw: "OpenClaw",
  hermes: "Hermes",
};

function isAgentPlatform(value: string | null | undefined): value is AgentPlatform {
  return Boolean(value && agentPlatforms.includes(value as AgentPlatform));
}

function sessionKey(session: Pick<SessionMeta, "providerId" | "sessionId" | "sourcePath">) {
  return `${session.providerId}:${session.sessionId}:${session.sourcePath}`;
}

function formatTime(value: number | null | undefined, language: Language, t: Translator) {
  if (!value) {
    return t("sessions.unknown");
  }
  const date = new Date(value < 1_000_000_000_000 ? value * 1000 : value);
  return new Intl.DateTimeFormat(language, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(date);
}

function formatRelativeTime(value: number | null | undefined, t: Translator) {
  if (!value) {
    return t("sessions.unknown");
  }
  const ms = (value < 1_000_000_000_000 ? value * 1000 : value);
  const diff = Date.now() - ms;
  const minutes = Math.max(1, Math.round(diff / 60_000));
  if (minutes < 60) {
    return t("sessions.relativeMinutes", { count: minutes });
  }
  const hours = Math.round(minutes / 60);
  if (hours < 48) {
    return t("sessions.relativeHours", { count: hours });
  }
  const days = Math.round(hours / 24);
  return t("sessions.relativeDays", { count: days });
}

function summarize(text: string, limit = 160) {
  const compact = text.replace(/\s+/g, " ").trim();
  return compact.length > limit ? `${compact.slice(0, limit)}...` : compact;
}

function titleForSession(session: SessionMeta) {
  return session.title?.trim() || session.projectDir?.trim() || session.sessionId;
}

function providerLabel(providerId: string) {
  return isAgentPlatform(providerId) ? platformLabels[providerId] : providerId;
}

function directoryLabel(projectDir: string | null | undefined, unknownDirectory: string) {
  return projectDir?.trim() || unknownDirectory;
}

function messageRoleLabel(role: string, t: Translator) {
  switch (role.toLowerCase()) {
    case "user":
      return t("sessions.role.user");
    case "assistant":
      return t("sessions.role.assistant");
    case "system":
      return t("sessions.role.system");
    case "tool":
      return t("sessions.role.tool");
    case "developer":
      return t("sessions.role.developer");
    default:
      return role;
  }
}

function messageMatches(message: SessionMessage, query: string) {
  return (
    message.content.toLowerCase().includes(query) ||
    message.role.toLowerCase().includes(query)
  );
}

export function SessionsScreen({ initialPlatform = null }: SessionsScreenProps) {
  const { language, t } = useI18n();
  const [platform, setPlatform] = useState<AgentPlatform | "all">(
    isAgentPlatform(initialPlatform) ? initialPlatform : "all",
  );
  const [search, setSearch] = useState("");
  const [messageSearch, setMessageSearch] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [mobileDetailOpen, setMobileDetailOpen] = useState(false);
  const [copiedValue, setCopiedValue] = useState<string | null>(null);
  const [listMode, setListMode] = useState<ListMode>("grouped");
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(() => new Set());
  const [copyMenuOpen, setCopyMenuOpen] = useState(false);
  const [navigationOpen, setNavigationOpen] = useState(false);
  const [terminalOpening, setTerminalOpening] = useState(false);
  const [terminalError, setTerminalError] = useState<string | null>(null);
  const copyMenuRef = useRef<HTMLDivElement | null>(null);
  const navigationRef = useRef<HTMLDivElement | null>(null);
  const deferredSearch = useDeferredValue(search);
  const deferredMessageSearch = useDeferredValue(messageSearch);

  useEffect(() => {
    setPlatform(isAgentPlatform(initialPlatform) ? initialPlatform : "all");
  }, [initialPlatform]);

  const sessionsQuery = useQuery({
    queryKey: ["sessions"],
    queryFn: () => listSessions(null),
  });

  const sessions = sessionsQuery.data ?? [];
  const counts = useMemo(() => {
    const byPlatform = new Map<string, number>();
    for (const session of sessions) {
      byPlatform.set(session.providerId, (byPlatform.get(session.providerId) ?? 0) + 1);
    }
    return byPlatform;
  }, [sessions]);

  const filteredSessions = useMemo(() => {
    const needle = deferredSearch.trim().toLowerCase();
    return sessions.filter((session) => {
      if (platform !== "all" && session.providerId !== platform) {
        return false;
      }
      if (!needle) {
        return true;
      }
      return (
        session.sessionId.toLowerCase().includes(needle) ||
        (session.title ?? "").toLowerCase().includes(needle) ||
        (session.projectDir ?? "").toLowerCase().includes(needle) ||
        session.sourcePath.toLowerCase().includes(needle) ||
        (session.resumeCommand ?? "").toLowerCase().includes(needle)
      );
    });
  }, [deferredSearch, platform, sessions]);

  const groupedSessions = useMemo(() => {
    const providers = new Map<string, Map<string, SessionMeta[]>>();
    for (const session of filteredSessions) {
      const directories = providers.get(session.providerId) ?? new Map<string, SessionMeta[]>();
      const directory = directoryLabel(session.projectDir, t("sessions.unknownDirectory"));
      directories.set(directory, [...(directories.get(directory) ?? []), session]);
      providers.set(session.providerId, directories);
    }
    return Array.from(providers.entries()).map(([providerId, directories]) => ({
      providerId,
      directories: Array.from(directories.entries()).map(([directory, items]) => ({
        directory,
        items,
        latestAt: items.reduce<number | null>((latest, item) => {
          const value = item.lastActiveAt ?? item.createdAt ?? null;
          if (value === null) {
            return latest;
          }
          return latest === null ? value : Math.max(latest, value);
        }, null),
      })),
    }));
  }, [filteredSessions, t]);

  useEffect(() => {
    if (filteredSessions.length === 0) {
      setSelectedId(null);
      return;
    }
    if (!selectedId || !filteredSessions.some((session) => sessionKey(session) === selectedId)) {
      setSelectedId(sessionKey(filteredSessions[0]));
    }
  }, [filteredSessions, selectedId]);

  useEffect(() => {
    setNavigationOpen(false);
    setCopyMenuOpen(false);
    setTerminalError(null);
  }, [selectedId]);

  const selectedSession = filteredSessions.find((session) => sessionKey(session) === selectedId) ?? null;
  useEffect(() => {
    if (!selectedSession) {
      setMobileDetailOpen(false);
    }
  }, [selectedSession]);

  const messagesQuery = useQuery({
    queryKey: ["session-messages", selectedSession?.providerId, selectedSession?.sourcePath],
    queryFn: () =>
      getSessionMessages({
        providerId: selectedSession!.providerId,
        sourcePath: selectedSession!.sourcePath,
      }),
    enabled: Boolean(selectedSession),
  });
  const messages = messagesQuery.data ?? [];
  const messageNeedle = deferredMessageSearch.trim().toLowerCase();
  const visibleMessages = useMemo(() => {
    if (!messageNeedle) {
      return messages.map((message, index) => ({ message, index }));
    }
    return messages
      .map((message, index) => ({ message, index }))
      .filter(({ message }) => messageMatches(message, messageNeedle));
  }, [messageNeedle, messages]);

  const tocItems = useMemo(() => {
    return messages
      .map((message, index) => ({ message, index }))
      .filter(({ message }) => message.role === "user" || messageMatches(message, messageNeedle))
      .slice(0, 36);
  }, [messageNeedle, messages]);

  useEffect(() => {
    if (!navigationOpen) {
      return;
    }
    const closeOnOutsidePointer = (event: PointerEvent) => {
      if (!navigationRef.current?.contains(event.target as Node)) {
        setNavigationOpen(false);
      }
    };
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setNavigationOpen(false);
      }
    };
    document.addEventListener("pointerdown", closeOnOutsidePointer);
    document.addEventListener("keydown", closeOnEscape);
    return () => {
      document.removeEventListener("pointerdown", closeOnOutsidePointer);
      document.removeEventListener("keydown", closeOnEscape);
    };
  }, [navigationOpen]);

  useEffect(() => {
    if (!copyMenuOpen) {
      return;
    }
    const closeOnOutsidePointer = (event: PointerEvent) => {
      if (!copyMenuRef.current?.contains(event.target as Node)) {
        setCopyMenuOpen(false);
      }
    };
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setCopyMenuOpen(false);
      }
    };
    document.addEventListener("pointerdown", closeOnOutsidePointer);
    document.addEventListener("keydown", closeOnEscape);
    return () => {
      document.removeEventListener("pointerdown", closeOnOutsidePointer);
      document.removeEventListener("keydown", closeOnEscape);
    };
  }, [copyMenuOpen]);

  useEffect(() => {
    if (!copiedValue) {
      return;
    }
    const timeout = window.setTimeout(() => setCopiedValue(null), 1500);
    return () => window.clearTimeout(timeout);
  }, [copiedValue]);

  const copyText = async (value: string | null | undefined, marker: string) => {
    if (!value) {
      return;
    }
    await navigator.clipboard.writeText(value);
    setCopiedValue(marker);
    setCopyMenuOpen(false);
  };

  const desktopRuntime = isDesktop();
  const canOpenTerminal = Boolean(
    desktopRuntime &&
      selectedSession?.projectDir?.trim() &&
      selectedSession.resumeCommand?.trim() &&
      !terminalOpening,
  );

  const openSelectedSessionTerminal = async () => {
    if (!selectedSession?.projectDir || !selectedSession.resumeCommand) {
      setTerminalError(t("sessions.openTerminalUnavailable"));
      return;
    }
    if (!desktopRuntime) {
      setTerminalError(t("sessions.openTerminalUnavailable"));
      return;
    }

    setTerminalError(null);
    setTerminalOpening(true);
    try {
      await openSessionTerminal({
        cwd: selectedSession.projectDir,
        command: selectedSession.resumeCommand,
      });
    } catch (error) {
      setTerminalError(error instanceof Error ? error.message : t("sessions.openTerminalError"));
    } finally {
      setTerminalOpening(false);
    }
  };

  const toggleGroup = (groupKey: string) => {
    setExpandedGroups((current) => {
      const next = new Set(current);
      if (next.has(groupKey)) {
        next.delete(groupKey);
      } else {
        next.add(groupKey);
      }
      return next;
    });
  };

  const changeListMode = (nextMode: ListMode) => {
    setListMode(nextMode);
    if (nextMode === "grouped") {
      setExpandedGroups(new Set());
    }
  };

  const renderSessionItem = (session: SessionMeta, showTime = true) => {
    const active = sessionKey(session) === selectedId;
    return (
      <button
        key={sessionKey(session)}
        className={`w-full cursor-pointer rounded-xl border p-3 text-left transition-colors ${
          active
            ? "border-emerald-700 bg-emerald-950 text-white shadow-sm"
            : "border-stone-200 bg-white/88 hover:border-emerald-200 hover:bg-emerald-50/70"
        }`}
        onClick={() => {
          setSelectedId(sessionKey(session));
          setMobileDetailOpen(true);
        }}
        type="button"
      >
        <div className="flex items-start justify-between gap-3">
          <div className="min-w-0">
            <p className={`truncate text-[13px] font-semibold ${active ? "text-white" : "text-stone-950"}`}>
              {titleForSession(session)}
            </p>
            <p className={`mt-0.5 truncate text-[12px] ${active ? "text-emerald-100" : "text-stone-500"}`}>
              {providerLabel(session.providerId)} · {directoryLabel(session.projectDir, t("sessions.unknownDirectory"))}
            </p>
          </div>
          {showTime && (
            <span className={`shrink-0 rounded-full px-2 py-0.5 text-[11px] font-semibold ${
              active ? "bg-white/12 text-white" : "bg-stone-100 text-stone-600"
            }`}>
              {formatRelativeTime(session.lastActiveAt ?? session.createdAt, t)}
            </span>
          )}
        </div>
      </button>
    );
  };

  return (
    <section className="grid gap-3 md:h-full md:min-h-0 md:grid-cols-[minmax(360px,0.9fr)_minmax(0,1.35fr)]">
      <div className={`min-w-0 space-y-3 md:min-h-0 md:flex md:flex-col ${mobileDetailOpen ? "hidden" : "flex flex-col"}`}>
        <div className="flex flex-wrap items-center justify-between gap-3 rounded-2xl border border-stone-200 bg-white/84 p-4 shadow-sm">
          <h1 className="text-xl font-semibold tracking-tight text-stone-950">{t("sessions.title")}</h1>
          <div className="flex shrink-0 items-center gap-1 rounded-xl border border-stone-200 bg-stone-50 p-1">
            <button
              aria-pressed={listMode === "grouped"}
              className={`inline-flex cursor-pointer items-center gap-1 rounded-lg px-2.5 py-1.5 text-[11px] font-semibold transition-colors ${
                listMode === "grouped"
                  ? "bg-emerald-700 text-white shadow-sm"
                  : "text-stone-600 hover:bg-white hover:text-stone-950"
              }`}
              onClick={() => changeListMode("grouped")}
              type="button"
            >
              <ListTree className="h-3.5 w-3.5" />
              {t("sessions.grouped")}
            </button>
            <button
              aria-pressed={listMode === "flat"}
              className={`inline-flex cursor-pointer items-center gap-1 rounded-lg px-2.5 py-1.5 text-[11px] font-semibold transition-colors ${
                listMode === "flat"
                  ? "bg-emerald-700 text-white shadow-sm"
                  : "text-stone-600 hover:bg-white hover:text-stone-950"
              }`}
              onClick={() => changeListMode("flat")}
              type="button"
            >
              <Rows3 className="h-3.5 w-3.5" />
              {t("sessions.flat")}
            </button>
          </div>
        </div>

        <div className="rounded-2xl border border-stone-200 bg-white/84 p-3 shadow-sm">
          <div className="flex flex-col gap-2 sm:flex-row">
            <label className="flex min-w-0 flex-1 items-center gap-2 rounded-xl border border-stone-200 bg-stone-50 px-3 py-2 text-[13px] text-stone-500">
              <Search className="h-4 w-4 shrink-0" />
              <input
                className="w-full bg-transparent outline-none"
                onChange={(event) => setSearch(event.target.value)}
                placeholder={t("sessions.searchPlaceholder")}
                value={search}
              />
            </label>
            <label className="flex w-28 shrink-0 items-center rounded-xl border border-stone-200 bg-white px-3 py-2 text-[13px] font-semibold text-stone-700">
              <span className="sr-only">{t("sessions.agentFilter")}</span>
              <select
                aria-label={t("sessions.agentFilter")}
                className="w-full cursor-pointer bg-transparent outline-none"
                onChange={(event) => setPlatform(event.target.value as AgentPlatform | "all")}
                value={platform}
              >
                <option value="all">{t("sessions.all")} · {sessions.length}</option>
                {agentPlatforms.map((item) => (
                  <option key={item} value={item}>
                    {platformLabels[item]} · {counts.get(item) ?? 0}
                  </option>
                ))}
              </select>
            </label>
          </div>

        </div>

        <div className="rounded-2xl border border-stone-200 bg-white/84 p-2 shadow-sm md:flex md:min-h-0 md:flex-1">
          <div className="max-h-[62vh] space-y-2 overflow-auto p-1 md:max-h-none md:min-h-0 md:flex-1">
            {sessionsQuery.isLoading && <p className="p-3 text-sm text-stone-500">{t("sessions.loading")}</p>}
            {!sessionsQuery.isLoading && filteredSessions.length === 0 && (
              <p className="p-3 text-sm text-stone-500">{t("sessions.noMatches")}</p>
            )}

            {listMode === "flat" && filteredSessions.map((session) => renderSessionItem(session))}
            {listMode === "grouped" &&
              groupedSessions.map((providerGroup) => (
                providerGroup.directories.map((directoryGroup) => {
                  const groupKey = `${providerGroup.providerId}:${directoryGroup.directory}`;
                  const expanded = expandedGroups.has(groupKey);
                  return (
                    <div key={groupKey} className="space-y-2 rounded-2xl border border-stone-200 bg-stone-50/80 p-2">
                      <button
                        aria-controls={`session-group-${groupKey}`}
                        aria-expanded={expanded}
                        className="flex w-full cursor-pointer items-center justify-between rounded-xl px-1 py-1 text-left transition-colors hover:bg-white/70"
                        onClick={() => toggleGroup(groupKey)}
                        type="button"
                      >
                        <span className="flex min-w-0 items-center gap-1.5">
                          {expanded ? (
                            <ChevronDown className="h-3.5 w-3.5 shrink-0 text-stone-500" />
                          ) : (
                            <ChevronRight className="h-3.5 w-3.5 shrink-0 text-stone-500" />
                          )}
                          <FolderOpen className="h-3.5 w-3.5 shrink-0 text-stone-500" />
                          <span className="truncate text-[12px] font-semibold text-stone-950">
                            {directoryGroup.directory}
                          </span>
                        </span>
                        <span className="shrink-0 text-[11px] text-stone-500">
                          {formatRelativeTime(directoryGroup.latestAt, t)}
                        </span>
                      </button>
                      {expanded && (
                        <div id={`session-group-${groupKey}`} className="space-y-2">
                          {directoryGroup.items.map((session) => renderSessionItem(session, false))}
                        </div>
                      )}
                    </div>
                  );
                })
              ))}
          </div>
        </div>
      </div>

      <div className={`min-w-0 space-y-3 md:min-h-0 md:flex md:flex-col md:overflow-hidden md:pr-1 ${mobileDetailOpen ? "flex flex-col" : "hidden"}`}>
        {selectedSession ? (
          <>
            <div className="rounded-2xl border border-stone-200 bg-white/86 p-4 shadow-sm">
              <button
                className="mb-3 inline-flex cursor-pointer items-center gap-1.5 rounded-xl border border-stone-200 bg-white px-3 py-2 text-[13px] font-semibold text-stone-700 transition-colors hover:bg-stone-50 md:hidden"
                onClick={() => setMobileDetailOpen(false)}
                type="button"
              >
                <ArrowLeft className="h-4 w-4" />
                {t("sessions.backToList")}
              </button>
              <div className="flex flex-wrap items-start justify-between gap-3">
                <div className="min-w-0">
                  <h2 className="truncate text-xl font-semibold tracking-tight text-stone-950">
                    {titleForSession(selectedSession)}
                  </h2>
                </div>
                <div className="flex shrink-0 items-center gap-2">
                  <span className="text-[12px] text-stone-500">
                    {t("sessions.updated")} {formatTime(selectedSession.lastActiveAt, language, t)}
                  </span>
                  <button
                    aria-label={t("sessions.openTerminal")}
                    className="grid h-9 w-9 cursor-pointer place-items-center rounded-xl border border-stone-200 bg-white text-stone-700 transition-colors hover:border-emerald-200 hover:bg-emerald-50 disabled:cursor-not-allowed disabled:opacity-45 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-emerald-500"
                    disabled={!canOpenTerminal}
                    onClick={() => void openSelectedSessionTerminal()}
                    title={
                      terminalOpening
                        ? t("sessions.openTerminalOpening")
                        : canOpenTerminal
                          ? t("sessions.openTerminal")
                          : t("sessions.openTerminalUnavailable")
                    }
                    type="button"
                  >
                    <Play className="h-4 w-4" />
                  </button>
                  <div className="relative" ref={copyMenuRef}>
                    <button
                      aria-expanded={copyMenuOpen}
                      aria-haspopup="menu"
                      aria-label={t("sessions.copyMenu")}
                      className={`grid h-9 w-9 cursor-pointer place-items-center rounded-xl border transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-emerald-500 ${
                        copyMenuOpen
                          ? "border-emerald-700 bg-emerald-700 text-white"
                          : "border-stone-200 bg-white text-stone-700 hover:border-emerald-200 hover:bg-emerald-50"
                      }`}
                      onClick={() => setCopyMenuOpen((open) => !open)}
                      title={t("sessions.copyMenu")}
                      type="button"
                    >
                      <MoreHorizontal className="h-4 w-4" />
                    </button>
                    {copyMenuOpen && (
                      <div
                        aria-label={t("sessions.copyMenu")}
                        className="absolute right-0 top-full z-30 mt-2 min-w-48 rounded-2xl border border-stone-200 bg-white p-1 shadow-xl shadow-stone-900/15"
                        role="menu"
                      >
                        <button
                          className="flex w-full cursor-pointer items-center gap-2 rounded-xl px-3 py-2 text-left text-[12px] font-semibold text-stone-700 transition-colors hover:bg-stone-50 disabled:cursor-not-allowed disabled:opacity-50"
                          disabled={!selectedSession.projectDir}
                          onClick={() => void copyText(selectedSession.projectDir, "project")}
                          role="menuitem"
                          type="button"
                        >
                          {copiedValue === "project" ? <Check className="h-4 w-4" /> : <FolderOpen className="h-4 w-4" />}
                          {copiedValue === "project" ? t("sessions.copied") : t("sessions.copyDirectory")}
                        </button>
                        <button
                          className="flex w-full cursor-pointer items-center gap-2 rounded-xl px-3 py-2 text-left text-[12px] font-semibold text-stone-700 transition-colors hover:bg-stone-50"
                          onClick={() => void copyText(selectedSession.sourcePath, "source")}
                          role="menuitem"
                          type="button"
                        >
                          {copiedValue === "source" ? <Check className="h-4 w-4" /> : <Copy className="h-4 w-4" />}
                          {copiedValue === "source" ? t("sessions.copied") : t("sessions.copySource")}
                        </button>
                        {selectedSession.resumeCommand && (
                          <button
                            className="flex w-full cursor-pointer items-center gap-2 rounded-xl px-3 py-2 text-left text-[12px] font-semibold text-stone-700 transition-colors hover:bg-stone-50"
                            onClick={() => void copyText(selectedSession.resumeCommand, "resume")}
                            role="menuitem"
                            type="button"
                          >
                            {copiedValue === "resume" ? <Check className="h-4 w-4" /> : <Terminal className="h-4 w-4" />}
                            {copiedValue === "resume" ? t("sessions.copied") : t("sessions.copyResume")}
                          </button>
                        )}
                      </div>
                    )}
                  </div>
                </div>
              </div>
              {terminalError && (
                <p aria-live="polite" className="mt-3 rounded-xl border border-red-200 bg-red-50 px-3 py-2 text-[12px] font-medium text-red-700">
                  {terminalError}
                </p>
              )}

            </div>

            <div className="rounded-2xl border border-stone-200 bg-white/86 p-3 shadow-sm md:flex md:min-h-0 md:flex-1 md:flex-col">
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <div className="flex items-center gap-2 text-[12px] font-semibold text-stone-500">
                    <FileText className="h-4 w-4" />
                    {t("sessions.messageTimeline")} · {visibleMessages.length}/{messages.length}
                  </div>
                  <div className="flex min-w-0 flex-1 items-center justify-end gap-2 sm:flex-none">
                    <label className="flex min-w-0 flex-1 items-center gap-2 rounded-xl border border-stone-200 bg-stone-50 px-3 py-2 text-[13px] text-stone-500 sm:min-w-[220px]">
                      <Search className="h-4 w-4 shrink-0" />
                      <input
                        className="w-full bg-transparent outline-none"
                        onChange={(event) => setMessageSearch(event.target.value)}
                        placeholder={t("sessions.searchMessages")}
                        value={messageSearch}
                      />
                    </label>
                    <div className="relative shrink-0" ref={navigationRef}>
                      <button
                        aria-expanded={navigationOpen}
                        aria-haspopup="dialog"
                        aria-label={t("sessions.quickNavigation")}
                        className={`inline-flex shrink-0 cursor-pointer items-center gap-1.5 rounded-xl border px-3 py-2 text-[12px] font-semibold transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-emerald-500 ${
                          navigationOpen
                            ? "border-emerald-700 bg-emerald-700 text-white"
                            : "border-stone-200 bg-white text-stone-700 hover:border-emerald-200 hover:bg-emerald-50"
                        }`}
                        onClick={() => setNavigationOpen((open) => !open)}
                        title={t("sessions.quickNavigation")}
                        type="button"
                      >
                        <Layers3 className="h-4 w-4" />
                        <span className="hidden sm:inline">{t("sessions.quickNavigation")}</span>
                      </button>
                      {navigationOpen && (
                        <div
                          aria-label={t("sessions.quickNavigation")}
                          className="absolute right-0 top-full z-30 mt-2 w-80 max-w-[calc(100vw-2rem)] rounded-2xl border border-stone-200 bg-white p-3 shadow-xl shadow-stone-900/15"
                          role="dialog"
                        >
                          <div className="flex items-center justify-between gap-3">
                            <div className="flex items-center gap-2 text-[12px] font-semibold text-stone-700">
                              <Layers3 className="h-4 w-4 text-emerald-700" />
                              {t("sessions.quickNavigation")}
                            </div>
                            <span className="text-[11px] text-stone-400">{tocItems.length}</span>
                          </div>
                          <div className="mt-3 max-h-[min(28rem,calc(100dvh-8rem))] space-y-1.5 overflow-y-auto pr-1">
                            {tocItems.length === 0 && <p className="text-[12px] text-stone-500">{t("sessions.noNavigation")}</p>}
                            {tocItems.map(({ message, index }, tocIndex) => (
                              <a
                                className="flex items-start gap-2 rounded-xl border border-stone-200 bg-stone-50 px-2 py-2 text-[12px] text-stone-600 transition-colors hover:border-emerald-200 hover:bg-emerald-50"
                                href={`#session-message-${index}`}
                                key={`${message.role}-${message.ts ?? index}-${tocIndex}`}
                                onClick={() => setNavigationOpen(false)}
                              >
                                <span className="grid h-5 w-5 shrink-0 place-items-center rounded-full bg-white text-[10px] font-semibold text-stone-500">
                                  {tocIndex + 1}
                                </span>
                                <span className="min-w-0">
                                  <span className="flex items-center gap-1 text-[11px] font-semibold uppercase text-stone-400">
                                    {message.role === "user" ? <MessageSquareText className="h-3 w-3" /> : <Hash className="h-3 w-3" />}
                                    {messageRoleLabel(message.role, t)}
                                  </span>
                                  <span className="mt-0.5 block max-h-10 overflow-hidden">{summarize(message.content, 96)}</span>
                                </span>
                              </a>
                            ))}
                          </div>
                        </div>
                      )}
                    </div>
                  </div>
                </div>

                <div className="mt-3 max-h-[58vh] space-y-2 overflow-auto pr-1 md:min-h-0 md:max-h-none md:flex-1">
                  {messagesQuery.isLoading && <p className="text-sm text-stone-500">{t("sessions.loadingMessages")}</p>}
                  {!messagesQuery.isLoading && visibleMessages.length === 0 && (
                    <p className="text-sm text-stone-500">{t("sessions.noMessageMatches")}</p>
                  )}
                  {visibleMessages.map(({ message, index }) => (
                    <article
                      className={`rounded-xl border px-3 py-2 ${
                        message.role === "user"
                          ? "border-blue-200 bg-blue-50"
                          : message.role === "assistant"
                            ? "border-emerald-200 bg-emerald-50"
                            : "border-stone-200 bg-white"
                      }`}
                      id={`session-message-${index}`}
                      key={`${message.role}-${message.ts ?? index}-${index}`}
                    >
                      <div className="flex items-center justify-between gap-3">
                        <p className="text-[11px] font-semibold uppercase tracking-wide text-stone-500">
                          {messageRoleLabel(message.role, t)}
                        </p>
                        <p className="text-[11px] text-stone-400">{formatTime(message.ts, language, t)}</p>
                      </div>
                      <p className="mt-1 whitespace-pre-wrap break-words text-[13px] leading-5 text-stone-800">
                        {message.content}
                      </p>
                    </article>
                  ))}
                </div>
              </div>
          </>
        ) : (
          <div className="grid min-h-[55vh] place-items-center rounded-2xl border border-dashed border-stone-200 bg-white/80 text-center">
            <div>
              <Clock className="mx-auto h-8 w-8 text-stone-300" />
              <p className="mt-2 text-sm font-semibold text-stone-950">{t("sessions.noSelection")}</p>
              <p className="mt-1 text-[13px] text-stone-500">{t("sessions.noSelectionBody")}</p>
            </div>
          </div>
        )}
      </div>
    </section>
  );
}
