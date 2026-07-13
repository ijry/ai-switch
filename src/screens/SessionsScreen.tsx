import { useQuery } from "@tanstack/react-query";
import {
  Check,
  Clock,
  Copy,
  FileText,
  FolderOpen,
  Hash,
  Layers3,
  ListTree,
  MessageSquareText,
  Rows3,
  Search,
  Terminal,
} from "lucide-react";
import { useDeferredValue, useEffect, useMemo, useState } from "react";
import { agentPlatforms, type AgentPlatform, agentScreenByPlatform } from "../components/layout/AppLayout";
import { getSessionMessages, listSessions } from "../lib/api/client";
import type { SessionMessage, SessionMeta } from "../lib/api/types";

type SessionsScreenProps = {
  initialPlatform?: string | null;
};

type ListMode = "flat" | "grouped";

const platformLabels: Record<AgentPlatform, string> = {
  codex: "Codex",
  claude: "Claude",
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

function formatTime(value?: number | null) {
  if (!value) {
    return "Unknown";
  }
  const date = new Date(value < 1_000_000_000_000 ? value * 1000 : value);
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(date);
}

function formatRelativeTime(value?: number | null) {
  if (!value) {
    return "Unknown";
  }
  const ms = (value < 1_000_000_000_000 ? value * 1000 : value);
  const diff = Date.now() - ms;
  const minutes = Math.max(1, Math.round(diff / 60_000));
  if (minutes < 60) {
    return `${minutes}m ago`;
  }
  const hours = Math.round(minutes / 60);
  if (hours < 48) {
    return `${hours}h ago`;
  }
  const days = Math.round(hours / 24);
  return `${days}d ago`;
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

function directoryLabel(projectDir?: string | null) {
  return projectDir?.trim() || "Unknown directory";
}

function messageMatches(message: SessionMessage, query: string) {
  return (
    message.content.toLowerCase().includes(query) ||
    message.role.toLowerCase().includes(query)
  );
}

export function SessionsScreen({ initialPlatform = null }: SessionsScreenProps) {
  const [platform, setPlatform] = useState<AgentPlatform | "all">(
    isAgentPlatform(initialPlatform) ? initialPlatform : "all",
  );
  const [search, setSearch] = useState("");
  const [messageSearch, setMessageSearch] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [copiedValue, setCopiedValue] = useState<string | null>(null);
  const [listMode, setListMode] = useState<ListMode>("grouped");
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
      const directory = directoryLabel(session.projectDir);
      directories.set(directory, [...(directories.get(directory) ?? []), session]);
      providers.set(session.providerId, directories);
    }
    return Array.from(providers.entries()).map(([providerId, directories]) => ({
      providerId,
      directories: Array.from(directories.entries()).map(([directory, items]) => ({ directory, items })),
    }));
  }, [filteredSessions]);

  useEffect(() => {
    if (filteredSessions.length === 0) {
      setSelectedId(null);
      return;
    }
    if (!selectedId || !filteredSessions.some((session) => sessionKey(session) === selectedId)) {
      setSelectedId(sessionKey(filteredSessions[0]));
    }
  }, [filteredSessions, selectedId]);

  const selectedSession = filteredSessions.find((session) => sessionKey(session) === selectedId) ?? null;
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
  };

  const renderSessionItem = (session: SessionMeta) => {
    const active = sessionKey(session) === selectedId;
    return (
      <button
        key={sessionKey(session)}
        className={`w-full cursor-pointer rounded-xl border p-3 text-left transition-colors ${
          active
            ? "border-emerald-700 bg-emerald-950 text-white shadow-sm"
            : "border-stone-200 bg-white/88 hover:border-emerald-200 hover:bg-emerald-50/70"
        }`}
        onClick={() => setSelectedId(sessionKey(session))}
        type="button"
      >
        <div className="flex items-start justify-between gap-3">
          <div className="min-w-0">
            <p className={`truncate text-[13px] font-semibold ${active ? "text-white" : "text-stone-950"}`}>
              {titleForSession(session)}
            </p>
            <p className={`mt-0.5 truncate text-[12px] ${active ? "text-emerald-100" : "text-stone-500"}`}>
              {providerLabel(session.providerId)} · {directoryLabel(session.projectDir)}
            </p>
          </div>
          <span className={`shrink-0 rounded-full px-2 py-0.5 text-[11px] font-semibold ${
            active ? "bg-white/12 text-white" : "bg-stone-100 text-stone-600"
          }`}>
            {formatRelativeTime(session.lastActiveAt ?? session.createdAt)}
          </span>
        </div>
        <p className={`mt-2 max-h-10 overflow-hidden text-[12px] ${active ? "text-emerald-50" : "text-stone-500"}`}>
          {session.resumeCommand ?? session.sourcePath}
        </p>
      </button>
    );
  };

  return (
    <section className="grid gap-3 2xl:grid-cols-[430px_minmax(0,1fr)]">
      <div className="space-y-3">
        <div className="rounded-2xl border border-stone-200 bg-white/84 p-4 shadow-sm">
          <p className="text-[11px] font-semibold uppercase tracking-wide text-emerald-700">Session manager</p>
          <h1 className="mt-0.5 text-xl font-semibold tracking-tight text-stone-950">Local agent sessions</h1>
          <p className="mt-1 text-[13px] text-stone-600">
            Search local transcripts, recover the right project directory, and copy resume commands.
          </p>
        </div>

        <div className="rounded-2xl border border-stone-200 bg-white/84 p-3 shadow-sm">
          <label className="flex items-center gap-2 rounded-xl border border-stone-200 bg-stone-50 px-3 py-2 text-[13px] text-stone-500">
            <Search className="h-4 w-4 shrink-0" />
            <input
              className="w-full bg-transparent outline-none"
              onChange={(event) => setSearch(event.target.value)}
              placeholder="Search title, directory, command, or session id"
              value={search}
            />
          </label>

          <div className="mt-3 flex flex-wrap gap-2">
            <button
              className={`cursor-pointer rounded-full border px-3 py-1 text-[12px] font-semibold transition-colors ${
                platform === "all"
                  ? "border-stone-900 bg-stone-900 text-white"
                  : "border-stone-200 bg-white text-stone-600 hover:border-stone-300"
              }`}
              onClick={() => setPlatform("all")}
              type="button"
            >
              All · {sessions.length}
            </button>
            {agentPlatforms.map((item) => (
              <button
                key={item}
                className={`cursor-pointer rounded-full border px-3 py-1 text-[12px] font-semibold transition-colors ${
                  platform === item
                    ? "border-stone-900 bg-stone-900 text-white"
                    : "border-stone-200 bg-white text-stone-600 hover:border-stone-300"
                }`}
                onClick={() => setPlatform(item)}
                type="button"
              >
                {platformLabels[item]} · {counts.get(item) ?? 0}
              </button>
            ))}
          </div>

          <div className="mt-3 flex items-center gap-2">
            <button
              className={`inline-flex cursor-pointer items-center gap-1.5 rounded-xl border px-3 py-2 text-[12px] font-semibold transition-colors ${
                listMode === "grouped"
                  ? "border-emerald-700 bg-emerald-700 text-white"
                  : "border-stone-200 bg-white text-stone-700 hover:bg-stone-50"
              }`}
              onClick={() => setListMode("grouped")}
              type="button"
            >
              <ListTree className="h-3.5 w-3.5" />
              Grouped
            </button>
            <button
              className={`inline-flex cursor-pointer items-center gap-1.5 rounded-xl border px-3 py-2 text-[12px] font-semibold transition-colors ${
                listMode === "flat"
                  ? "border-emerald-700 bg-emerald-700 text-white"
                  : "border-stone-200 bg-white text-stone-700 hover:bg-stone-50"
              }`}
              onClick={() => setListMode("flat")}
              type="button"
            >
              <Rows3 className="h-3.5 w-3.5" />
              Flat
            </button>
          </div>
        </div>

        <div className="rounded-2xl border border-stone-200 bg-white/84 p-2 shadow-sm">
          <div className="max-h-[62vh] space-y-2 overflow-auto p-1">
            {sessionsQuery.isLoading && <p className="p-3 text-sm text-stone-500">Loading sessions...</p>}
            {!sessionsQuery.isLoading && filteredSessions.length === 0 && (
              <p className="p-3 text-sm text-stone-500">No sessions matched the current filters.</p>
            )}

            {listMode === "flat" && filteredSessions.map(renderSessionItem)}
            {listMode === "grouped" &&
              groupedSessions.map((providerGroup) => (
                <div key={providerGroup.providerId} className="space-y-2 rounded-2xl border border-stone-200 bg-stone-50/80 p-2">
                  <div className="flex items-center justify-between px-1">
                    <p className="text-[12px] font-semibold text-stone-950">{providerLabel(providerGroup.providerId)}</p>
                    <p className="text-[11px] text-stone-500">
                      {providerGroup.directories.reduce((total, group) => total + group.items.length, 0)} sessions
                    </p>
                  </div>
                  {providerGroup.directories.map((directoryGroup) => (
                    <div key={`${providerGroup.providerId}:${directoryGroup.directory}`} className="space-y-1.5">
                      <div className="flex items-center gap-1.5 px-1 text-[11px] font-semibold uppercase tracking-wide text-stone-400">
                        <FolderOpen className="h-3.5 w-3.5" />
                        <span className="truncate normal-case tracking-normal">{directoryGroup.directory}</span>
                      </div>
                      {directoryGroup.items.map(renderSessionItem)}
                    </div>
                  ))}
                </div>
              ))}
          </div>
        </div>
      </div>

      <div className="min-w-0 space-y-3">
        {selectedSession ? (
          <>
            <div className="rounded-2xl border border-stone-200 bg-white/86 p-4 shadow-sm">
              <div className="flex flex-wrap items-start justify-between gap-3">
                <div className="min-w-0">
                  <p className="text-[11px] font-semibold uppercase tracking-wide text-stone-400">Selected session</p>
                  <h2 className="truncate text-xl font-semibold tracking-tight text-stone-950">
                    {titleForSession(selectedSession)}
                  </h2>
                  <p className="mt-1 text-[13px] text-stone-500">
                    {providerLabel(selectedSession.providerId)} · {selectedSession.sessionId}
                  </p>
                </div>
                <div className="flex flex-wrap items-center gap-2">
                  <button
                    className="inline-flex cursor-pointer items-center gap-2 rounded-xl border border-stone-200 bg-white px-3 py-2 text-[13px] font-semibold text-stone-800 transition-colors hover:bg-stone-50 disabled:cursor-not-allowed disabled:opacity-50"
                    disabled={!selectedSession.projectDir}
                    onClick={() => copyText(selectedSession.projectDir, "project")}
                    type="button"
                  >
                    {copiedValue === "project" ? <Check className="h-4 w-4" /> : <FolderOpen className="h-4 w-4" />}
                    {copiedValue === "project" ? "Copied" : "Copy directory"}
                  </button>
                  <button
                    className="inline-flex cursor-pointer items-center gap-2 rounded-xl border border-stone-200 bg-white px-3 py-2 text-[13px] font-semibold text-stone-800 transition-colors hover:bg-stone-50"
                    onClick={() => copyText(selectedSession.sourcePath, "source")}
                    type="button"
                  >
                    {copiedValue === "source" ? <Check className="h-4 w-4" /> : <Copy className="h-4 w-4" />}
                    {copiedValue === "source" ? "Copied" : "Copy source"}
                  </button>
                  {selectedSession.resumeCommand && (
                    <button
                      className="inline-flex cursor-pointer items-center gap-2 rounded-xl bg-stone-900 px-3 py-2 text-[13px] font-semibold text-white transition-colors hover:bg-stone-800"
                      onClick={() => copyText(selectedSession.resumeCommand, "resume")}
                      type="button"
                    >
                      {copiedValue === "resume" ? <Check className="h-4 w-4" /> : <Terminal className="h-4 w-4" />}
                      {copiedValue === "resume" ? "Copied" : "Copy resume"}
                    </button>
                  )}
                </div>
              </div>

              <div className="mt-4 grid gap-2 md:grid-cols-4">
                <div className="rounded-xl border border-stone-200 bg-stone-50 px-3 py-2">
                  <p className="text-[11px] uppercase tracking-wide text-stone-400">Provider</p>
                  <p className="mt-1 truncate text-[13px] font-semibold text-stone-950">
                    {agentScreenByPlatform[selectedSession.providerId as AgentPlatform] ?? selectedSession.providerId}
                  </p>
                </div>
                <div className="rounded-xl border border-stone-200 bg-stone-50 px-3 py-2">
                  <p className="text-[11px] uppercase tracking-wide text-stone-400">Project</p>
                  <p className="mt-1 truncate text-[13px] font-semibold text-stone-950">
                    {directoryLabel(selectedSession.projectDir)}
                  </p>
                </div>
                <div className="rounded-xl border border-stone-200 bg-stone-50 px-3 py-2">
                  <p className="text-[11px] uppercase tracking-wide text-stone-400">Created</p>
                  <p className="mt-1 text-[13px] font-semibold text-stone-950">
                    {formatTime(selectedSession.createdAt)}
                  </p>
                </div>
                <div className="rounded-xl border border-stone-200 bg-stone-50 px-3 py-2">
                  <p className="text-[11px] uppercase tracking-wide text-stone-400">Updated</p>
                  <p className="mt-1 text-[13px] font-semibold text-stone-950">
                    {formatTime(selectedSession.lastActiveAt)}
                  </p>
                </div>
              </div>

              <div className="mt-3 rounded-2xl border border-stone-200 bg-stone-50 p-3">
                <p className="break-all font-mono text-[11px] text-stone-700">
                  {selectedSession.resumeCommand ?? "No resume command available."}
                </p>
                <p className="mt-2 break-all font-mono text-[11px] text-stone-500">{selectedSession.sourcePath}</p>
              </div>
            </div>

            <div className="grid gap-3 xl:grid-cols-[minmax(0,1fr)_260px]">
              <div className="rounded-2xl border border-stone-200 bg-white/86 p-3 shadow-sm">
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <div className="flex items-center gap-2 text-[12px] font-semibold text-stone-500">
                    <FileText className="h-4 w-4" />
                    Message timeline · {visibleMessages.length}/{messages.length}
                  </div>
                  <label className="flex min-w-[260px] items-center gap-2 rounded-xl border border-stone-200 bg-stone-50 px-3 py-2 text-[13px] text-stone-500">
                    <Search className="h-4 w-4 shrink-0" />
                    <input
                      className="w-full bg-transparent outline-none"
                      onChange={(event) => setMessageSearch(event.target.value)}
                      placeholder="Search inside this session"
                      value={messageSearch}
                    />
                  </label>
                </div>

                <div className="mt-3 max-h-[58vh] space-y-2 overflow-auto pr-1">
                  {messagesQuery.isLoading && <p className="text-sm text-stone-500">Loading messages...</p>}
                  {!messagesQuery.isLoading && visibleMessages.length === 0 && (
                    <p className="text-sm text-stone-500">No messages matched the current search.</p>
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
                          {message.role}
                        </p>
                        <p className="text-[11px] text-stone-400">{formatTime(message.ts)}</p>
                      </div>
                      <p className="mt-1 max-h-56 overflow-auto whitespace-pre-wrap text-[13px] leading-5 text-stone-800">
                        {message.content}
                      </p>
                    </article>
                  ))}
                </div>
              </div>

              <aside className="rounded-2xl border border-stone-200 bg-white/86 p-3 shadow-sm">
                <div className="flex items-center gap-2 text-[12px] font-semibold text-stone-500">
                  <Layers3 className="h-4 w-4" />
                  Quick navigation
                </div>
                <div className="mt-3 max-h-[58vh] space-y-1.5 overflow-auto">
                  {tocItems.length === 0 && <p className="text-[12px] text-stone-500">No navigation items.</p>}
                  {tocItems.map(({ message, index }, tocIndex) => (
                    <a
                      className="flex items-start gap-2 rounded-xl border border-stone-200 bg-stone-50 px-2 py-2 text-[12px] text-stone-600 transition-colors hover:border-emerald-200 hover:bg-emerald-50"
                      href={`#session-message-${index}`}
                      key={`${message.role}-${message.ts ?? index}-${tocIndex}`}
                    >
                      <span className="grid h-5 w-5 shrink-0 place-items-center rounded-full bg-white text-[10px] font-semibold text-stone-500">
                        {tocIndex + 1}
                      </span>
                      <span className="min-w-0">
                        <span className="flex items-center gap-1 text-[11px] font-semibold uppercase text-stone-400">
                          {message.role === "user" ? <MessageSquareText className="h-3 w-3" /> : <Hash className="h-3 w-3" />}
                          {message.role}
                        </span>
                        <span className="mt-0.5 block max-h-10 overflow-hidden">{summarize(message.content, 96)}</span>
                      </span>
                    </a>
                  ))}
                </div>
              </aside>
            </div>
          </>
        ) : (
          <div className="grid min-h-[55vh] place-items-center rounded-2xl border border-dashed border-stone-200 bg-white/80 text-center">
            <div>
              <Clock className="mx-auto h-8 w-8 text-stone-300" />
              <p className="mt-2 text-sm font-semibold text-stone-950">No session selected</p>
              <p className="mt-1 text-[13px] text-stone-500">Pick a session to inspect its transcript and recovery data.</p>
            </div>
          </div>
        )}
      </div>
    </section>
  );
}
