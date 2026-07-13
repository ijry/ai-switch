import { useQuery } from "@tanstack/react-query";
import { Check, Copy, ExternalLink, FileText, Search, Terminal } from "lucide-react";
import { useDeferredValue, useEffect, useMemo, useState } from "react";
import { getSessionMessages, listSessions } from "../lib/api/client";
import { agentPlatforms, type AgentPlatform, agentScreenByPlatform } from "../components/layout/AppLayout";

const platformLabels: Record<AgentPlatform, string> = {
  codex: "Codex",
  claude: "Claude",
  gemini: "Gemini",
  opencode: "OpenCode",
  openclaw: "OpenClaw",
  hermes: "Hermes",
};

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

function summarize(text: string, limit = 160) {
  const compact = text.replace(/\s+/g, " ").trim();
  return compact.length > limit ? `${compact.slice(0, limit)}...` : compact;
}

function sessionKey(session: { providerId: string; sessionId: string; sourcePath: string }) {
  return `${session.providerId}:${session.sessionId}:${session.sourcePath}`;
}

export function SessionsScreen() {
  const [platform, setPlatform] = useState<AgentPlatform | "all">("all");
  const [search, setSearch] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [copiedPath, setCopiedPath] = useState<string | null>(null);
  const deferredSearch = useDeferredValue(search);

  const sessionsQuery = useQuery({
    queryKey: ["sessions", platform],
    queryFn: () => listSessions(platform === "all" ? null : platform),
  });

  const sessions = sessionsQuery.data ?? [];
  const filteredSessions = useMemo(() => {
    const needle = deferredSearch.trim().toLowerCase();
    if (!needle) {
      return sessions;
    }

    return sessions.filter((session) => {
      return (
        session.sessionId.toLowerCase().includes(needle) ||
        (session.title ?? "").toLowerCase().includes(needle) ||
        (session.projectDir ?? "").toLowerCase().includes(needle) ||
        session.sourcePath.toLowerCase().includes(needle)
      );
    });
  }, [deferredSearch, sessions]);

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

  useEffect(() => {
    if (!copiedPath) {
      return;
    }

    const timeout = window.setTimeout(() => setCopiedPath(null), 1500);
    return () => window.clearTimeout(timeout);
  }, [copiedPath]);

  const counts = useMemo(() => {
    const byPlatform = new Map<string, number>();
    for (const session of sessions) {
      byPlatform.set(session.providerId, (byPlatform.get(session.providerId) ?? 0) + 1);
    }
    return byPlatform;
  }, [sessions]);

  return (
    <section className="grid gap-3 xl:grid-cols-[380px_minmax(0,1fr)]">
      <div className="space-y-3">
        <div className="rounded-2xl border border-stone-200 bg-white/82 p-4 shadow-sm">
          <p className="text-[11px] font-semibold uppercase tracking-wide text-stone-400">Sessions</p>
          <h1 className="mt-0.5 text-lg font-semibold tracking-tight text-stone-950">Local session index</h1>
          <p className="mt-1 text-[13px] text-stone-600">
            Scan recent agent sessions, open message previews, and reuse the resume command.
          </p>
        </div>

        <div className="rounded-2xl border border-stone-200 bg-white/82 p-3 shadow-sm">
          <label className="flex items-center gap-2 rounded-xl border border-stone-200 bg-stone-50 px-3 py-2 text-[13px] text-stone-500">
            <Search className="h-4 w-4 shrink-0" />
            <input
              className="w-full bg-transparent outline-none"
              onChange={(event) => setSearch(event.target.value)}
              placeholder="Search title, path, or session id"
              value={search}
            />
          </label>

          <div className="mt-3 flex flex-wrap gap-2">
            <button
              className={`rounded-full border px-3 py-1 text-[12px] font-semibold transition-colors ${
                platform === "all"
                  ? "border-stone-900 bg-stone-900 text-white"
                  : "border-stone-200 bg-white text-stone-600 hover:border-stone-300"
              }`}
              onClick={() => setPlatform("all")}
              type="button"
            >
              All
            </button>
            {agentPlatforms.map((item) => (
              <button
                key={item}
                className={`rounded-full border px-3 py-1 text-[12px] font-semibold transition-colors ${
                  platform === item
                    ? "border-stone-900 bg-stone-900 text-white"
                    : "border-stone-200 bg-white text-stone-600 hover:border-stone-300"
                }`}
                onClick={() => setPlatform(item)}
                type="button"
              >
                {platformLabels[item]}
              </button>
            ))}
          </div>

          <div className="mt-4 grid gap-2 sm:grid-cols-2">
            {agentPlatforms.map((item) => (
              <div key={item} className="rounded-xl border border-stone-200 bg-stone-50 px-3 py-2">
                <p className="text-[12px] font-semibold text-stone-950">{platformLabels[item]}</p>
                <p className="mt-0.5 text-[12px] text-stone-500">{counts.get(item) ?? 0} sessions</p>
              </div>
            ))}
          </div>
        </div>

        <div className="rounded-2xl border border-stone-200 bg-white/82 p-2 shadow-sm">
          <div className="max-h-[56vh] space-y-2 overflow-auto p-1">
            {sessionsQuery.isLoading && <p className="p-3 text-sm text-stone-500">Loading sessions...</p>}
            {!sessionsQuery.isLoading && filteredSessions.length === 0 && (
              <p className="p-3 text-sm text-stone-500">No sessions matched the current filters.</p>
            )}

            {filteredSessions.map((session) => {
              const active = sessionKey(session) === selectedId;
              return (
                <button
                  key={sessionKey(session)}
                  className={`w-full rounded-xl border p-3 text-left transition-colors ${
                    active ? "border-stone-900 bg-stone-900 text-white" : "border-stone-200 bg-white hover:bg-stone-50"
                  }`}
                  onClick={() => setSelectedId(sessionKey(session))}
                  type="button"
                >
                  <div className="flex items-start justify-between gap-3">
                    <div className="min-w-0">
                      <p className={`truncate text-[13px] font-semibold ${active ? "text-white" : "text-stone-950"}`}>
                        {session.title?.trim() || session.sessionId}
                      </p>
                      <p className={`mt-0.5 truncate text-[12px] ${active ? "text-stone-300" : "text-stone-500"}`}>
                        {agentScreenByPlatform[session.providerId as AgentPlatform]} · {session.projectDir ?? "No project"}
                      </p>
                    </div>
                    <span className={`rounded-full px-2 py-0.5 text-[11px] font-semibold ${
                      active ? "bg-white/12 text-white" : "bg-stone-100 text-stone-600"
                    }`}>
                      {formatTime(session.lastActiveAt ?? session.createdAt)}
                    </span>
                  </div>
                  <p className={`mt-2 max-h-10 overflow-hidden text-[12px] ${active ? "text-stone-200" : "text-stone-500"}`}>
                    {session.resumeCommand ?? "Resume command unavailable."}
                  </p>
                </button>
              );
            })}
          </div>
        </div>
      </div>

      <div className="space-y-3">
        <div className="rounded-2xl border border-stone-200 bg-white/82 p-4 shadow-sm">
          {selectedSession ? (
            <div className="space-y-4">
              <div className="flex flex-wrap items-start justify-between gap-3">
                <div className="min-w-0">
                  <p className="text-[11px] font-semibold uppercase tracking-wide text-stone-400">Selected session</p>
                  <h2 className="truncate text-xl font-semibold tracking-tight text-stone-950">
                    {selectedSession.title?.trim() || selectedSession.sessionId}
                  </h2>
                  <p className="mt-1 text-[13px] text-stone-500">
                    {platformLabels[selectedSession.providerId as AgentPlatform]} · {selectedSession.sessionId}
                  </p>
                </div>
                <div className="flex items-center gap-2">
                  <button
                    className="inline-flex items-center gap-2 rounded-xl border border-stone-200 bg-white px-3 py-2 text-[13px] font-semibold text-stone-800 transition-colors hover:bg-stone-50"
                    onClick={async () => {
                      await navigator.clipboard.writeText(selectedSession.sourcePath);
                      setCopiedPath(selectedSession.sourcePath);
                    }}
                    type="button"
                  >
                    {copiedPath === selectedSession.sourcePath ? <Check className="h-4 w-4" /> : <Copy className="h-4 w-4" />}
                    {copiedPath === selectedSession.sourcePath ? "Copied" : "Copy path"}
                  </button>
                  {selectedSession.resumeCommand && (
                    <button
                      className="inline-flex items-center gap-2 rounded-xl bg-stone-900 px-3 py-2 text-[13px] font-semibold text-white transition-colors hover:bg-stone-800"
                      onClick={async () => {
                        await navigator.clipboard.writeText(selectedSession.resumeCommand ?? "");
                      }}
                      type="button"
                    >
                      <Terminal className="h-4 w-4" />
                      Copy resume
                    </button>
                  )}
                </div>
              </div>

              <div className="grid gap-2 sm:grid-cols-3">
                <div className="rounded-xl border border-stone-200 bg-stone-50 px-3 py-2">
                  <p className="text-[11px] uppercase tracking-wide text-stone-400">Project</p>
                  <p className="mt-1 truncate text-[13px] font-semibold text-stone-950">
                    {selectedSession.projectDir ?? "Unknown"}
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

              <div className="rounded-2xl border border-stone-200 bg-stone-50 p-3">
                <div className="flex items-center gap-2 text-[12px] font-semibold text-stone-500">
                  <FileText className="h-4 w-4" />
                  Message preview
                </div>
                <div className="mt-3 space-y-2">
                  {messagesQuery.isLoading && <p className="text-sm text-stone-500">Loading messages...</p>}
                  {!messagesQuery.isLoading && messages.length === 0 && (
                    <p className="text-sm text-stone-500">No message preview could be parsed from this file.</p>
                  )}
                  {messages.map((message, index) => (
                    <div
                      className={`rounded-xl border px-3 py-2 ${
                        message.role === "user"
                          ? "border-blue-200 bg-blue-50"
                          : message.role === "assistant"
                            ? "border-emerald-200 bg-emerald-50"
                            : "border-stone-200 bg-white"
                      }`}
                      key={`${message.role}-${message.ts ?? index}`}
                    >
                      <div className="flex items-center justify-between gap-3">
                        <p className="text-[11px] font-semibold uppercase tracking-wide text-stone-500">
                          {message.role}
                        </p>
                        <p className="text-[11px] text-stone-400">{formatTime(message.ts)}</p>
                      </div>
                      <p className="mt-1 max-h-40 overflow-auto whitespace-pre-wrap text-[13px] leading-5 text-stone-800">
                        {summarize(message.content, 400)}
                      </p>
                    </div>
                  ))}
                </div>
              </div>

              <div className="rounded-2xl border border-stone-200 bg-stone-50 p-3 text-[12px] text-stone-500">
                <div className="flex items-center gap-2">
                  <ExternalLink className="h-4 w-4" />
                  Source path
                </div>
                <p className="mt-2 break-all font-mono text-[11px] text-stone-700">{selectedSession.sourcePath}</p>
                <p className="mt-2 break-all font-mono text-[11px] text-stone-600">
                  {selectedSession.resumeCommand ?? "No resume command available."}
                </p>
              </div>
            </div>
          ) : (
            <div className="grid min-h-[42vh] place-items-center rounded-2xl border border-dashed border-stone-200 bg-stone-50 text-center">
              <div>
                <p className="text-sm font-semibold text-stone-950">No session selected</p>
                <p className="mt-1 text-[13px] text-stone-500">Pick a session on the left to inspect its messages.</p>
              </div>
            </div>
          )}
        </div>
      </div>
    </section>
  );
}
