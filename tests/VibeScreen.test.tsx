import { QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../src/App";
import {
  createTerminalSession,
  killTerminalSession,
  listSessions,
} from "../src/lib/api/client";
import type { SessionMeta, TerminalSession } from "../src/lib/api/types";
import { createQueryClient } from "../src/lib/query/queryClient";
import { VibeScreen } from "../src/screens/VibeScreen";

vi.mock("../src/components/terminal/XtermPane", () => ({
  XtermPane: ({ session }: { session: TerminalSession }) => (
    <div data-testid={`terminal-pane-${session.id}`}>{session.title}</div>
  ),
}));

vi.mock("../src/screens/AccountsScreen", () => ({
  AccountsScreen: () => <div>Agent accounts placeholder</div>,
}));

vi.mock("../src/lib/api/client", () => ({
  createTerminalSession: vi.fn(),
  killTerminalSession: vi.fn(),
  listSessions: vi.fn(),
}));

const sessions: SessionMeta[] = [
  {
    providerId: "codex",
    sessionId: "s1",
    title: "Fix terminal bug",
    projectDir: "D:/repo/app",
    createdAt: 1,
    lastActiveAt: 2,
    sourcePath: "D:/repo/app/session.jsonl",
    resumeCommand: "codex resume s1",
  },
  {
    providerId: "claude",
    sessionId: "s2",
    title: "Missing resume",
    projectDir: "D:/repo/app",
    createdAt: 1,
    lastActiveAt: 2,
    sourcePath: "D:/repo/app/session-2.jsonl",
    resumeCommand: null,
  },
];

function renderScreen() {
  return render(
    <QueryClientProvider client={createQueryClient()}>
      <VibeScreen />
    </QueryClientProvider>,
  );
}

describe("VibeScreen", () => {
  beforeEach(() => {
    vi.mocked(createTerminalSession).mockReset();
    vi.mocked(killTerminalSession).mockReset();
    vi.mocked(listSessions).mockReset();
    vi.mocked(listSessions).mockResolvedValue(sessions);
    vi.mocked(createTerminalSession).mockResolvedValue({
      id: "term-1",
      title: "Fix terminal bug",
      platform: "codex",
      cwd: "D:/repo/app",
      command: "codex resume s1",
      status: "running",
      createdAt: 123,
    });
    vi.mocked(killTerminalSession).mockResolvedValue(undefined);
  });

  it("groups local sessions by project directory", async () => {
    renderScreen();

    expect(await screen.findByText("D:/repo/app")).toBeInTheDocument();
    expect(screen.getByText("Fix terminal bug")).toBeInTheDocument();
    expect(screen.getByText("Missing resume")).toBeInTheDocument();
  });

  it("launches a resume terminal from a complete session", async () => {
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: /Resume Fix terminal bug/ }));

    await waitFor(() =>
      expect(createTerminalSession).toHaveBeenCalledWith({
        kind: "resume",
        platform: "codex",
        command: "codex resume s1",
        title: "Fix terminal bug",
        cwd: "D:/repo/app",
        cols: 100,
        rows: 30,
      }),
    );
    expect(await screen.findByTestId("terminal-pane-term-1")).toBeInTheDocument();
  });

  it("does not launch a session without resume metadata", async () => {
    renderScreen();

    const disabled = await screen.findByRole("button", {
      name: /Cannot resume Missing resume/,
    });
    expect(disabled).toBeDisabled();
    expect(createTerminalSession).not.toHaveBeenCalled();
  });

  it("opens Vibe from the app navigation", async () => {
    render(<App />);

    await userEvent.click(screen.getByRole("button", { name: "Vibe" }));

    expect(await screen.findByText("Terminal workspace")).toBeInTheDocument();
  });
});
