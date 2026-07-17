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
import { __resetTransportForTests } from "../src/lib/transport";
import { VIBE_SKIN_STORAGE_KEY } from "../src/lib/vibeSkin";
import { VibeScreen } from "../src/screens/VibeScreen";

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
}));

vi.mock("../src/components/terminal/XtermPane", () => ({
  XtermPane: ({ session, themeOverride }: { session: TerminalSession; themeOverride?: unknown }) => (
    <div data-testid={`terminal-pane-${session.id}`} data-theme-override={themeOverride ? "yes" : "no"}>
      {session.title}
    </div>
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

async function expandProjectDirectory() {
  await userEvent.click(await screen.findByRole("button", { name: "Expand folder D:/repo/app" }));
}

describe("VibeScreen", () => {
  beforeEach(() => {
    window.localStorage.clear();
    (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {};
    __resetTransportForTests();
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
    expect(screen.queryByText("Fix terminal bug")).not.toBeInTheDocument();
    expect(screen.queryByText("Missing resume")).not.toBeInTheDocument();

    await expandProjectDirectory();

    expect(screen.getByText("Fix terminal bug")).toBeInTheDocument();
    expect(screen.getByText("Missing resume")).toBeInTheDocument();
  });

  it("launches a resume terminal from a complete session", async () => {
    renderScreen();

    await expandProjectDirectory();
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

    await expandProjectDirectory();
    const disabled = await screen.findByRole("button", {
      name: /Cannot resume Missing resume/,
    });
    expect(disabled).toBeDisabled();
    expect(createTerminalSession).not.toHaveBeenCalled();
  });

  it("opens Vibe from the app navigation", async () => {
    render(<App />);

    await userEvent.click(screen.getByRole("button", { name: "Switch to Vibe mode" }));

    expect(
      await screen.findByRole("heading", { name: "Terminal workspace · Vibe mode" }),
    ).toBeInTheDocument();
    expect(screen.queryByText("Agent accounts placeholder")).not.toBeInTheDocument();
  });

  it("cycles Vibe through dark, light, and skin themes", async () => {
    renderScreen();

    const themeButton = await screen.findByRole("button", { name: "Switch Vibe theme" });
    expect(screen.getByText("Solarized Dark")).toBeInTheDocument();

    await userEvent.click(themeButton);

    expect(screen.getByText("Light")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Expand folder D:/repo/app" })).toHaveClass("text-stone-800");
    expect(screen.getByText("No terminal tabs yet.").parentElement).toHaveClass("bg-white/85");
    expect(screen.getByText("Start or resume a session")).toHaveClass("text-stone-900");

    await userEvent.click(themeButton);

    expect(screen.getByText("Skin")).toBeInTheDocument();
    expect(screen.getByLabelText("Vibe skin")).toHaveValue("codex-2007-blue");
    expect(screen.getByText("No terminal tabs yet.").parentElement).toHaveClass("vibe-skin-tabbar");
  });

  it("imports a custom Vibe skin package and applies its terminal theme", async () => {
    renderScreen();

    const skinFile = new File(
      [
        JSON.stringify({
          id: "custom-neon",
          name: "Custom Neon",
          ui: {
            accent: "#00ffee",
            text: "#f4fbff",
            mutedText: "#9be7ff",
            background: "linear-gradient(135deg, #001018, #06405a)",
            panel: "rgba(2, 28, 40, 0.78)",
            panelStrong: "rgba(4, 42, 58, 0.92)",
            panelSubtle: "rgba(0, 255, 238, 0.12)",
            border: "rgba(0, 255, 238, 0.35)",
            button: "linear-gradient(180deg, #00ffee, #0088ff)",
            buttonHover: "linear-gradient(180deg, #54fff5, #2da4ff)",
            tabActive: "rgba(0, 255, 238, 0.22)",
          },
          terminal: {
            background: "#010203",
            foreground: "#eafcff",
          },
        }),
      ],
      "custom.aiskin",
      { type: "application/json" },
    );

    await userEvent.upload(screen.getByLabelText("Choose Vibe skin package"), skinFile);

    expect(await screen.findByText("Skin")).toBeInTheDocument();
    expect(screen.getByLabelText("Vibe skin")).toHaveValue("custom-neon");
    expect(window.localStorage.getItem(VIBE_SKIN_STORAGE_KEY)).toContain("Custom Neon");

    await expandProjectDirectory();
    await userEvent.click(await screen.findByRole("button", { name: /Resume Fix terminal bug/ }));

    expect(await screen.findByTestId("terminal-pane-term-1")).toHaveAttribute(
      "data-theme-override",
      "yes",
    );
  });

  it("creates a new agent session through the modal", async () => {
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "New session" }));
    await screen.findByRole("dialog", { name: "Create session" });
    await userEvent.selectOptions(screen.getByLabelText("Existing folder"), "D:/repo/app");
    await userEvent.selectOptions(screen.getByLabelText("Agent"), "claude");
    await userEvent.click(screen.getByRole("button", { name: "Create" }));

    await waitFor(() =>
      expect(createTerminalSession).toHaveBeenCalledWith({
        kind: "agent",
        platform: "claude",
        command: null,
        title: "claude - D:/repo/app",
        cwd: "D:/repo/app",
        cols: 100,
        rows: 30,
      }),
    );
  });
});
