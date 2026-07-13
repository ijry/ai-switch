import { useQuery } from "@tanstack/react-query";
import { Bot, FolderOpen, Play, Plus, Terminal, X } from "lucide-react";
import { useCallback, useMemo, useState } from "react";
import {
  createTerminalSession,
  killTerminalSession,
  listSessions,
} from "../lib/api/client";
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

function titleForSession(session: SessionMeta) {
  return session.title?.trim() || session.projectDir?.trim() || session.sessionId;
}

function directoryLabel(session: SessionMeta) {
  return session.projectDir?.trim() || "Unknown directory";
}

function groupSessions(sessions: SessionMeta[]) {
  const groups = new Map<string, SessionMeta[]>();
  for (const session of sessions) {
    const directory = directoryLabel(session);
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

export function VibeScreen() {
  const [tabs, setTabs] = useState<TerminalSession[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [projectDir, setProjectDir] = useState("");
  const [platform, setPlatform] = useState<(typeof agentOptions)[number]["platform"]>("codex");
  const [error, setError] = useState<string | null>(null);

  const sessionsQuery = useQuery({
    queryKey: ["sessions"],
    queryFn: () => listSessions(null),
  });

  const groupedSessions = useMemo(
    () => groupSessions(sessionsQuery.data ?? []),
    [sessionsQuery.data],
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
      setError("This session is missing a project directory or resume command.");
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

  const launchNew = (kind: "agent" | "shell") => {
    const cwd = projectDir.trim();
    if (!cwd) {
      setError("Project directory is required.");
      return;
    }

    void openTerminal({
      kind,
      platform: kind === "agent" ? platform : null,
      command: null,
      title: kind === "agent" ? `${platform} - ${cwd}` : `Shell - ${cwd}`,
      cwd,
      cols: 100,
      rows: 30,
    });
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

  const activeTab = tabs.find((tab) => tab.id === activeId) ?? null;

  return (
    <section className="grid min-h-[calc(100vh-2rem)] gap-3 xl:grid-cols-[360px_minmax(0,1fr)]">
      <aside className="overflow-hidden rounded-3xl border border-zinc-800 bg-zinc-950 text-zinc-100 shadow-xl">
        <div className="border-b border-zinc-800 p-4">
          <p className="text-[11px] font-semibold uppercase tracking-[0.28em] text-amber-400">
            Vibe mode
          </p>
          <h1 className="mt-1 text-2xl font-semibold tracking-tight">
            Terminal workspace
          </h1>
          <p className="mt-2 text-[13px] leading-5 text-zinc-400">
            Start agents, resume local transcripts, and keep each workflow in its own tab.
          </p>
        </div>

        <div className="space-y-3 border-b border-zinc-800 p-4">
          <label className="block text-[12px] font-semibold text-zinc-400">
            Project directory
            <input
              className="mt-1 w-full rounded-xl border border-zinc-700 bg-zinc-900 px-3 py-2 text-[13px] text-zinc-100 outline-none focus:border-amber-400"
              onChange={(event) => setProjectDir(event.target.value)}
              placeholder="D:/Repos/project"
              value={projectDir}
            />
          </label>

          <label className="block text-[12px] font-semibold text-zinc-400">
            Agent
            <select
              className="mt-1 w-full rounded-xl border border-zinc-700 bg-zinc-900 px-3 py-2 text-[13px] text-zinc-100 outline-none focus:border-amber-400"
              onChange={(event) =>
                setPlatform(event.target.value as (typeof agentOptions)[number]["platform"])
              }
              value={platform}
            >
              {agentOptions.map((option) => (
                <option key={option.platform} value={option.platform}>
                  {option.label}
                </option>
              ))}
            </select>
          </label>

          <div className="grid grid-cols-2 gap-2">
            <button
              className="inline-flex items-center justify-center gap-2 rounded-xl bg-amber-400 px-3 py-2 text-[13px] font-semibold text-zinc-950 transition hover:bg-amber-300"
              onClick={() => launchNew("agent")}
              type="button"
            >
              <Bot className="h-4 w-4" />
              New agent
            </button>
            <button
              className="inline-flex items-center justify-center gap-2 rounded-xl border border-zinc-700 bg-zinc-900 px-3 py-2 text-[13px] font-semibold text-zinc-100 transition hover:border-amber-400"
              onClick={() => launchNew("shell")}
              type="button"
            >
              <Plus className="h-4 w-4" />
              Shell
            </button>
          </div>

          {error && (
            <p className="rounded-xl border border-red-400/40 bg-red-950/60 p-2 text-[12px] text-red-100">
              {error}
            </p>
          )}
        </div>

        <div className="max-h-[58vh] space-y-3 overflow-auto p-3">
          {sessionsQuery.isLoading && (
            <p className="text-sm text-zinc-400">Loading local sessions...</p>
          )}
          {!sessionsQuery.isLoading && groupedSessions.length === 0 && (
            <p className="rounded-2xl border border-zinc-800 bg-zinc-900/70 p-3 text-sm text-zinc-400">
              No local sessions found.
            </p>
          )}
          {groupedSessions.map((group) => (
            <div
              className="rounded-2xl border border-zinc-800 bg-zinc-900/70 p-2"
              key={group.directory}
            >
              <div className="mb-2 flex items-center gap-2 px-1 text-[12px] font-semibold text-zinc-300">
                <FolderOpen className="h-4 w-4 text-amber-300" />
                <span className="truncate">{group.directory}</span>
              </div>
              <div className="space-y-1.5">
                {group.items.map((session) => {
                  const canResume = Boolean(session.projectDir && session.resumeCommand);
                  const title = titleForSession(session);
                  return (
                    <button
                      aria-label={canResume ? `Resume ${title}` : `Cannot resume ${title}`}
                      className="w-full rounded-xl border border-zinc-800 bg-zinc-950/70 px-3 py-2 text-left text-[13px] text-zinc-200 transition hover:border-amber-400/60 disabled:cursor-not-allowed disabled:opacity-45"
                      disabled={!canResume}
                      key={sessionKey(session)}
                      onClick={() => resumeSession(session)}
                      type="button"
                    >
                      <span className="flex items-center justify-between gap-2">
                        <span className="truncate font-semibold">{title}</span>
                        <Play className="h-3.5 w-3.5 shrink-0 text-amber-300" />
                      </span>
                      <span className="mt-0.5 block truncate text-[11px] text-zinc-500">
                        {session.providerId} - {session.resumeCommand ?? "missing resume command"}
                      </span>
                    </button>
                  );
                })}
              </div>
            </div>
          ))}
        </div>
      </aside>

      <div className="min-w-0 overflow-hidden rounded-3xl border border-zinc-800 bg-[#10100f] shadow-xl">
        <div className="flex min-h-14 items-center gap-2 overflow-x-auto border-b border-zinc-800 bg-zinc-950 px-3">
          {tabs.length === 0 && (
            <p className="text-[13px] text-zinc-500">No terminal tabs yet.</p>
          )}
          {tabs.map((tab) => (
            <div
              className={`inline-flex max-w-[280px] items-center gap-1 rounded-xl border p-1 ${
                activeId === tab.id
                  ? "border-amber-400 bg-amber-400 text-zinc-950"
                  : "border-zinc-800 bg-zinc-900 text-zinc-300"
              }`}
              key={tab.id}
            >
              <button
                className="inline-flex min-w-0 items-center gap-2 px-2 py-1 text-[12px] font-semibold"
                onClick={() => setActiveId(tab.id)}
                type="button"
              >
                <Terminal className="h-3.5 w-3.5 shrink-0" />
                <span className="truncate">{tab.title}</span>
                <span className="rounded-full bg-black/20 px-1.5 py-0.5 text-[10px]">
                  {tab.status}
                </span>
              </button>
              <button
                aria-label={`Close ${tab.title}`}
                className="rounded-lg p-1 transition hover:bg-black/10"
                onClick={() => void closeTab(tab)}
                type="button"
              >
                <X className="h-3.5 w-3.5" />
              </button>
            </div>
          ))}
        </div>

        <div className="h-[calc(100vh-7rem)] p-3">
          {!activeTab && (
            <div className="grid h-full place-items-center rounded-2xl border border-dashed border-zinc-800 text-center">
              <div>
                <Terminal className="mx-auto h-8 w-8 text-zinc-700" />
                <p className="mt-2 text-sm font-semibold text-zinc-300">
                  Start or resume a session
                </p>
                <p className="mt-1 text-[13px] text-zinc-500">
                  The terminal will appear here.
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
            />
          ))}
        </div>
      </div>
    </section>
  );
}
