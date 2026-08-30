import { QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { act } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../src/App";
import {
  createTerminalSession,
  killTerminalSession,
  listAgentLaunchOptions,
  listSessions,
} from "../src/lib/api/client";
import type { AgentLaunchOption, SessionMeta, TerminalSession } from "../src/lib/api/types";
import { createQueryClient } from "../src/lib/query/queryClient";
import { __resetTransportForTests } from "../src/lib/transport";
import { VIBE_APPEARANCE_STORAGE_KEY, VIBE_SKIN_STORAGE_KEY } from "../src/lib/vibeSkin";
import { VIBE_TABS_STORAGE_KEY } from "../src/lib/vibeTabs";
import { VibeScreen } from "../src/screens/VibeScreen";

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
}));

vi.mock("../src/components/terminal/XtermPane", () => ({
  XtermPane: ({
    session,
    themeOverride,
    transparentSurface,
    onStatusChange,
  }: {
    session: TerminalSession;
    themeOverride?: unknown;
    transparentSurface?: boolean;
    onStatusChange?: (sessionId: string, status: string, exitCode?: number | null) => void;
  }) => (
    <div
      data-testid={`terminal-pane-${session.id}`}
      data-theme-override={themeOverride ? "yes" : "no"}
      data-transparent-surface={transparentSurface ? "yes" : "no"}
    >
      {session.title}
      <button
        data-testid={`terminal-exit-clean-${session.id}`}
        onClick={() => onStatusChange?.(session.id, "exited", 0)}
        type="button"
      />
      <button
        data-testid={`terminal-exit-fail-${session.id}`}
        onClick={() => onStatusChange?.(session.id, "exited", 3)}
        type="button"
      />
    </div>
  ),
}));

vi.mock("../src/screens/AccountsScreen", () => ({
  AccountsScreen: () => <div>Agent accounts placeholder</div>,
}));

vi.mock("../src/lib/api/client", () => ({
  createTerminalSession: vi.fn(),
  killTerminalSession: vi.fn(),
  listAgentLaunchOptions: vi.fn(),
  listSessions: vi.fn(),
}));

class MockAudioElement {
  static instances: MockAudioElement[] = [];

  currentTime = 0;
  loop = false;
  pause = vi.fn();
  play = vi.fn(() => Promise.resolve());
  src: string;
  volume = 1;

  constructor(src: string) {
    this.src = src;
    MockAudioElement.instances.push(this);
  }
}

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

function launchOption(overrides: Partial<AgentLaunchOption> & { platform: string }): AgentLaunchOption {
  return {
    displayName: overrides.platform,
    program: overrides.platform,
    installed: true,
    npmPackage: `@example/${overrides.platform}`,
    installCommand: `npm install -g @example/${overrides.platform}`,
    supportsModelSelection: false,
    supportsReasoning: false,
    models: [],
    ...overrides,
  };
}

const agentLaunchOptions: AgentLaunchOption[] = [
  launchOption({
    platform: "codex",
    displayName: "Codex",
    npmPackage: "@openai/codex",
    installCommand: "npm install -g @openai/codex",
    supportsModelSelection: true,
    supportsReasoning: true,
    models: [
      {
        id: "gpt-5.6-sol",
        reasoningLevels: [
          { effort: "medium", description: "Balanced" },
          { effort: "high", description: "Deep" },
        ],
        defaultReasoningLevel: "medium",
      },
      { id: "gpt-5.6-codex", reasoningLevels: [], defaultReasoningLevel: null },
    ],
  }),
  launchOption({
    platform: "claude",
    displayName: "Claude",
    npmPackage: "@anthropic-ai/claude-code",
    installCommand: "npm install -g @anthropic-ai/claude-code",
    supportsModelSelection: true,
    models: [{ id: "claude-sonnet-4-6", reasoningLevels: [], defaultReasoningLevel: null }],
  }),
  launchOption({
    platform: "grok",
    displayName: "Grok",
    installed: false,
    npmPackage: "@vibe-kit/grok-cli",
    installCommand: "npm install -g @vibe-kit/grok-cli",
    supportsModelSelection: true,
    models: [],
  }),
  launchOption({ platform: "gemini", displayName: "Gemini" }),
  launchOption({ platform: "opencode", displayName: "OpenCode" }),
  launchOption({ platform: "openclaw", displayName: "OpenClaw" }),
  launchOption({ platform: "hermes", displayName: "Hermes" }),
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

async function openAppearanceDialog() {
  await userEvent.click(await screen.findByRole("button", { name: "Switch Vibe theme" }));
  return screen.findByRole("dialog", { name: "Appearance" });
}

async function switchThemeFromAppearance(theme: "Solarized Dark" | "Light" | "Skin") {
  await openAppearanceDialog();
  await userEvent.click(await screen.findByRole("button", { name: theme }));
  await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
}

async function switchToSkinTheme() {
  await switchThemeFromAppearance("Skin");
}

function dispatchPointerEvent(
  target: EventTarget,
  type: "pointerdown" | "pointermove" | "pointerup",
  {
    button = 0,
    clientX = 0,
    pointerId = 1,
  }: { button?: number; clientX?: number; pointerId?: number } = {},
) {
  const event = new Event(type, { bubbles: true, cancelable: true });
  Object.defineProperties(event, {
    button: { configurable: true, value: button },
    clientX: { configurable: true, value: clientX },
    clientY: { configurable: true, value: 0 },
    pointerId: { configurable: true, value: pointerId },
  });
  act(() => {
    target.dispatchEvent(event);
  });
}

describe("VibeScreen", () => {
  beforeEach(() => {
    window.localStorage.clear();
    (window as Window & {
      __TAURI_INTERNALS__?: {
        invoke?: ReturnType<typeof vi.fn>;
        transformCallback?: ReturnType<typeof vi.fn>;
      };
    }).__TAURI_INTERNALS__ = {
      invoke: vi.fn().mockResolvedValue(1),
      transformCallback: vi.fn(() => 1),
    };
    MockAudioElement.instances = [];
    vi.stubGlobal("Audio", MockAudioElement);
    __resetTransportForTests();
    vi.mocked(createTerminalSession).mockReset();
    vi.mocked(killTerminalSession).mockReset();
    vi.mocked(listSessions).mockReset();
    vi.mocked(listAgentLaunchOptions).mockReset();
    vi.mocked(listSessions).mockResolvedValue(sessions);
    vi.mocked(listAgentLaunchOptions).mockResolvedValue(agentLaunchOptions);
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

    const folderToggle = await screen.findByRole("button", {
      name: "Expand folder D:/repo/app",
    });
    expect(folderToggle).toHaveTextContent("repo/app");
    expect(screen.queryByText("D:/repo/app")).not.toBeInTheDocument();
    expect(folderToggle).toHaveAttribute("title", "D:/repo/app");
    expect(screen.queryByText("Fix terminal bug")).not.toBeInTheDocument();
    expect(screen.queryByText("Missing resume")).not.toBeInTheDocument();

    await expandProjectDirectory();

    expect(screen.getByText("Fix terminal bug")).toBeInTheDocument();
    expect(screen.getByText("Missing resume")).toBeInTheDocument();
  });

  it("merges dated session directories by meaningful child folder", async () => {
    vi.mocked(listSessions).mockResolvedValue([
      {
        providerId: "codex",
        sessionId: "dated-1",
        title: "Older dated session",
        projectDir: "D:/repo/sessions/2026-05-24/project-alpha",
        createdAt: 1,
        lastActiveAt: 2,
        sourcePath: "D:/repo/sessions/2026-05-24/project-alpha/session.jsonl",
        resumeCommand: "codex resume dated-1",
      },
      {
        providerId: "codex",
        sessionId: "dated-2",
        title: "Newer dated session",
        projectDir: "D:/repo/sessions/2026-05-28/project-alpha",
        createdAt: 3,
        lastActiveAt: 4,
        sourcePath: "D:/repo/sessions/2026-05-28/project-alpha/session.jsonl",
        resumeCommand: "codex resume dated-2",
      },
    ]);
    renderScreen();

    const folderToggle = await screen.findByRole("button", {
      name: "Expand folder project-alpha",
    });
    expect(folderToggle).toHaveTextContent("project-alpha");
    expect(screen.queryByRole("button", { name: "Expand folder 2026-05-24/project-alpha" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Expand folder 2026-05-28/project-alpha" })).not.toBeInTheDocument();

    await userEvent.click(folderToggle);

    expect(screen.getByText("Older dated session")).toBeInTheDocument();
    expect(screen.getByText("Newer dated session")).toBeInTheDocument();
  });

  it("groups date and uuid session directories by day", async () => {
    vi.mocked(listSessions).mockResolvedValue([
      {
        providerId: "codex",
        sessionId: "opaque-1",
        title: "Opaque session",
        projectDir: "D:/repo/sessions/2026-05-24/123e4567-e89b-12d3-a456-426614174000",
        createdAt: 1,
        lastActiveAt: 2,
        sourcePath:
          "D:/repo/sessions/2026-05-24/123e4567-e89b-12d3-a456-426614174000/session.jsonl",
        resumeCommand: "codex resume opaque-1",
      },
    ]);
    renderScreen();

    const folderToggle = await screen.findByRole("button", {
      name: "Expand folder D:/repo/sessions/2026-05-24",
    });
    expect(folderToggle).toHaveTextContent("2026-05-24");
    expect(folderToggle).toHaveAttribute("title", "D:/repo/sessions/2026-05-24");
    expect(screen.queryByText("123e4567-e89b-12d3-a456-426614174000")).not.toBeInTheDocument();
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

  it("keeps Vibe terminals alive when switching back to the default mode", async () => {
    render(<App />);

    await userEvent.click(screen.getByRole("button", { name: "Switch to Vibe mode" }));
    await expandProjectDirectory();
    await userEvent.click(await screen.findByRole("button", { name: /Resume Fix terminal bug/ }));
    const pane = await screen.findByTestId("terminal-pane-term-1");

    await userEvent.click(screen.getByRole("button", { name: "Switch to Agent mode" }));

    expect(await screen.findByText("Agent accounts placeholder")).toBeInTheDocument();
    // The Vibe subtree stays mounted (hidden) so PTY-backed panes are not recreated.
    expect(pane).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Switch to Vibe mode" }));

    expect(screen.getByTestId("terminal-pane-term-1")).toBe(pane);
    expect(screen.getByRole("button", { name: "Fix terminal bug" })).toBeInTheDocument();
    expect(createTerminalSession).toHaveBeenCalledTimes(1);
  });

  it("restores the tabs that were open before the last shutdown", async () => {
    window.localStorage.setItem(
      VIBE_TABS_STORAGE_KEY,
      JSON.stringify([
        {
          input: {
            kind: "resume",
            platform: "codex",
            command: "codex resume s1",
            title: "Fix terminal bug",
            cwd: "D:/repo/app",
            cols: 100,
            rows: 30,
          },
          active: true,
        },
        // Unusable descriptors are dropped instead of failing the whole restore.
        { input: { kind: "resume", cwd: "D:/repo/app" } },
        { input: { kind: "shell", cwd: "   " } },
      ]),
    );

    renderScreen();

    expect(await screen.findByTestId("terminal-pane-term-1")).toBeInTheDocument();
    expect(createTerminalSession).toHaveBeenCalledTimes(1);
    expect(createTerminalSession).toHaveBeenCalledWith(
      expect.objectContaining({
        kind: "resume",
        platform: "codex",
        command: "codex resume s1",
        cwd: "D:/repo/app",
      }),
    );
    expect(screen.getByRole("button", { name: "Fix terminal bug" })).toBeInTheDocument();
  });

  it("remembers open tabs and forgets the ones that get closed", async () => {
    renderScreen();

    await expandProjectDirectory();
    await userEvent.click(await screen.findByRole("button", { name: /Resume Fix terminal bug/ }));
    await screen.findByTestId("terminal-pane-term-1");

    await waitFor(() => {
      const stored = window.localStorage.getItem(VIBE_TABS_STORAGE_KEY);
      expect(stored).toContain("codex resume s1");
      expect(stored).toContain('"active":true');
    });

    await userEvent.click(screen.getByRole("button", { name: "Close Fix terminal bug" }));

    await waitFor(() =>
      expect(window.localStorage.getItem(VIBE_TABS_STORAGE_KEY)).toBeNull(),
    );
  });

  it("hides sub-agent bookkeeping sessions from the session list", async () => {
    vi.mocked(listSessions).mockResolvedValue([
      ...sessions,
      {
        providerId: "claude",
        sessionId: "s3",
        title:
          "<local-command-caveat>Caveat: The messages below were generated while running local commands.</local-command-caveat>",
        projectDir: "D:/repo/app",
        createdAt: 1,
        lastActiveAt: 2,
        sourcePath: "D:/repo/app/session-3.jsonl",
        resumeCommand: "claude --resume s3",
      },
      {
        providerId: "claude",
        sessionId: "s4",
        title: "<command-name>/clear</command-name>",
        projectDir: "D:/repo/app",
        createdAt: 1,
        lastActiveAt: 2,
        sourcePath: "D:/repo/app/session-4.jsonl",
        resumeCommand: "claude --resume s4",
      },
    ]);
    renderScreen();

    await expandProjectDirectory();

    expect(screen.getByText("Fix terminal bug")).toBeInTheDocument();
    expect(screen.queryByText(/local-command-caveat/)).not.toBeInTheDocument();
    expect(screen.queryByText(/command-name/)).not.toBeInTheDocument();
  });

  it("collapses and restores the session list from the rail control", async () => {
    const view = renderScreen();

    await screen.findByRole("button", { name: "Expand folder D:/repo/app" });
    const collapse = screen.getByRole("button", { name: "Collapse session list" });
    expect(collapse).toHaveAttribute("aria-expanded", "true");

    await userEvent.click(collapse);

    expect(
      screen.queryByRole("button", { name: "Expand folder D:/repo/app" }),
    ).not.toBeInTheDocument();
    const expand = screen.getByRole("button", { name: "Expand session list" });
    expect(expand).toHaveAttribute("aria-expanded", "false");
    await waitFor(() =>
      expect(window.localStorage.getItem(VIBE_APPEARANCE_STORAGE_KEY)).toContain(
        '"sessionListCollapsed":true',
      ),
    );

    view.unmount();
    renderScreen();

    expect(
      await screen.findByRole("button", { name: "Expand session list" }),
    ).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Expand session list" }));

    expect(
      await screen.findByRole("button", { name: "Expand folder D:/repo/app" }),
    ).toBeInTheDocument();
  });

  it("resizes the session list from the rail drag gutter and persists the width", async () => {
    const view = renderScreen();

    await screen.findByRole("button", { name: "Expand folder D:/repo/app" });
    const shell = screen.getByRole("main");
    const handle = screen.getByTestId("vibe-session-resize-handle");
    expect(shell.style.getPropertyValue("--vibe-session-list-width")).toBe("356px");
    expect(handle).toHaveAttribute("role", "separator");

    dispatchPointerEvent(handle, "pointerdown", { clientX: 356, pointerId: 21 });
    dispatchPointerEvent(document, "pointermove", { clientX: 420, pointerId: 21 });
    expect(shell.style.getPropertyValue("--vibe-session-list-width")).toBe("420px");

    dispatchPointerEvent(document, "pointermove", { clientX: 20, pointerId: 21 });
    expect(shell.style.getPropertyValue("--vibe-session-list-width")).toBe("220px");

    dispatchPointerEvent(document, "pointermove", { clientX: 1200, pointerId: 21 });
    expect(shell.style.getPropertyValue("--vibe-session-list-width")).toBe("560px");
    dispatchPointerEvent(document, "pointerup", { pointerId: 21 });

    await waitFor(() =>
      expect(window.localStorage.getItem(VIBE_APPEARANCE_STORAGE_KEY)).toContain(
        '"sessionListWidth":560',
      ),
    );

    view.unmount();
    renderScreen();

    await screen.findByRole("button", { name: "Expand folder D:/repo/app" });
    expect(
      screen.getByRole("main").style.getPropertyValue("--vibe-session-list-width"),
    ).toBe("560px");
  });

  it("tiles terminals horizontally from the tab bar context menu", async () => {
    const view = renderScreen();

    await expandProjectDirectory();
    await userEvent.click(await screen.findByRole("button", { name: /Resume Fix terminal bug/ }));
    await screen.findByTestId("terminal-pane-term-1");
    expect(screen.queryByTestId("vibe-tiled-terminals")).not.toBeInTheDocument();

    const tabBar = screen.getByRole("button", { name: "Fix terminal bug" }).parentElement
      ?.parentElement as HTMLElement;
    await userEvent.pointer({ keys: "[MouseRight]", target: tabBar });

    const menu = screen.getByRole("menu", { name: "Terminal tab options" });
    await userEvent.click(
      screen.getByRole("menuitem", { name: "Enable tiled layout" }),
    );

    expect(menu).not.toBeInTheDocument();
    const tiled = screen.getByTestId("vibe-tiled-terminals");
    expect(tiled).toHaveClass("overflow-x-auto");
    expect(tiled).toContainElement(screen.getByTestId("terminal-pane-term-1"));
    expect(screen.getByTestId("terminal-pane-term-1").parentElement).toHaveClass(
      "vibe-tiled-terminal",
    );
    await waitFor(() =>
      expect(window.localStorage.getItem(VIBE_APPEARANCE_STORAGE_KEY)).toContain(
        '"tiledTerminals":true',
      ),
    );

    view.unmount();
    renderScreen();
    // The tab comes back from storage on mount, so there is nothing to resume here.
    await screen.findByTestId("vibe-tiled-terminals");

    await userEvent.pointer({
      keys: "[MouseRight]",
      target: screen.getByRole("button", { name: "Fix terminal bug" }).parentElement
        ?.parentElement as HTMLElement,
    });
    await userEvent.click(screen.getByRole("menuitem", { name: "Disable tiled layout" }));

    expect(screen.queryByTestId("vibe-tiled-terminals")).not.toBeInTheDocument();
  });

  it("keeps the tiled row scrollbar visible and applies the configured tile width", async () => {
    const view = renderScreen();

    await expandProjectDirectory();
    await userEvent.click(await screen.findByRole("button", { name: /Resume Fix terminal bug/ }));
    await screen.findByTestId("terminal-pane-term-1");

    await userEvent.pointer({
      keys: "[MouseRight]",
      target: screen.getByRole("button", { name: "Fix terminal bug" }).parentElement
        ?.parentElement as HTMLElement,
    });
    await userEvent.click(screen.getByRole("menuitem", { name: "Enable tiled layout" }));

    const tiled = screen.getByTestId("vibe-tiled-terminals");
    expect(tiled).toHaveClass("vibe-scrollbar-active");
    expect(screen.getByRole("main").style.getPropertyValue("--vibe-tile-width")).toBe("448px");

    await userEvent.click(screen.getByRole("button", { name: "Tab layout settings" }));
    const dialog = await screen.findByRole("dialog", { name: "Tab layout" });
    // Range inputs do not respond to typing, so the slider value is set directly.
    fireEvent.change(screen.getByLabelText("Tiled terminal width"), {
      target: { value: "640" },
    });

    expect(screen.getByRole("main").style.getPropertyValue("--vibe-tile-width")).toBe("640px");
    expect(dialog).toHaveTextContent("640 px");

    await waitFor(() =>
      expect(window.localStorage.getItem(VIBE_APPEARANCE_STORAGE_KEY)).toContain(
        '"tileWidth":640',
      ),
    );

    view.unmount();
    renderScreen();
    await screen.findByTestId("vibe-tiled-terminals");
    expect(screen.getByRole("main").style.getPropertyValue("--vibe-tile-width")).toBe("640px");
  });

  it("lets tabs size to their titles from the tab layout settings dialog", async () => {
    renderScreen();

    await expandProjectDirectory();
    await userEvent.click(await screen.findByRole("button", { name: /Resume Fix terminal bug/ }));
    const tab = (await screen.findByRole("button", { name: "Fix terminal bug" }))
      .parentElement as HTMLElement;
    expect(tab).toHaveClass("min-w-[132px]");

    await userEvent.click(screen.getByRole("button", { name: "Tab layout settings" }));
    await screen.findByRole("dialog", { name: "Tab layout" });
    await userEvent.click(screen.getByRole("checkbox", { name: "Fit tab width to its title" }));
    await userEvent.click(screen.getByRole("button", { name: "Close" }));

    expect(
      (screen.getByRole("button", { name: "Fix terminal bug" }).parentElement as HTMLElement)
        .className,
    ).not.toContain("min-w-[132px]");
    await waitFor(() =>
      expect(window.localStorage.getItem(VIBE_APPEARANCE_STORAGE_KEY)).toContain(
        '"tabWidthFitsContent":true',
      ),
    );
  });

  it("colours the tab status dot from the terminal exit code", async () => {
    renderScreen();

    await expandProjectDirectory();
    await userEvent.click(await screen.findByRole("button", { name: /Resume Fix terminal bug/ }));
    await screen.findByTestId("terminal-pane-term-1");

    const dot = screen.getByTestId("vibe-tab-status-term-1");
    expect(dot).toHaveClass("bg-emerald-400");

    await userEvent.click(screen.getByTestId("terminal-exit-fail-term-1"));
    expect(screen.getByTestId("vibe-tab-status-term-1")).toHaveClass("bg-amber-500");
    expect(
      screen.getByTestId("vibe-tab-status-term-1").closest("div[title]"),
    ).toHaveAttribute("title", "Fix terminal bug · exited · exit code 3");

    await userEvent.click(screen.getByTestId("terminal-exit-clean-term-1"));
    expect(screen.getByTestId("vibe-tab-status-term-1")).toHaveClass("bg-slate-400");
    expect(
      screen.getByTestId("vibe-tab-status-term-1").closest("div[title]"),
    ).toHaveAttribute("title", "Fix terminal bug · exited · exit code 0");
  });

  it("scrolls the tab strip with the arrow buttons once the tabs overflow", async () => {
    renderScreen();

    const strip = await screen.findByTestId("vibe-tab-strip");
    const scrollLeftArrow = screen.getByRole("button", { name: "Scroll tabs left" });
    const scrollRightArrow = screen.getByRole("button", { name: "Scroll tabs right" });

    // jsdom has no layout, so the strip metrics are stubbed to simulate overflow.
    expect(scrollLeftArrow).toBeDisabled();
    expect(scrollRightArrow).toBeDisabled();

    Object.defineProperty(strip, "clientWidth", { configurable: true, value: 400 });
    Object.defineProperty(strip, "scrollWidth", { configurable: true, value: 1200 });
    const scrollBy = vi.fn();
    Object.defineProperty(strip, "scrollBy", { configurable: true, value: scrollBy });
    fireEvent.scroll(strip);

    await waitFor(() => expect(scrollRightArrow).toBeEnabled());
    expect(scrollLeftArrow).toBeDisabled();

    await userEvent.click(scrollRightArrow);
    expect(scrollBy).toHaveBeenCalledWith({ behavior: "smooth", left: 280 });
    expect(strip).toHaveClass("vibe-scrollbar-active");

    strip.scrollLeft = 280;
    fireEvent.scroll(strip);

    await waitFor(() => expect(scrollLeftArrow).toBeEnabled());
    await userEvent.click(scrollLeftArrow);
    expect(scrollBy).toHaveBeenLastCalledWith({ behavior: "smooth", left: -280 });

    strip.scrollLeft = 800;
    fireEvent.scroll(strip);

    await waitFor(() => expect(scrollRightArrow).toBeDisabled());
  });

  it("renders the empty-state launch composer with agent and routing controls", async () => {
    renderScreen();

    expect(await screen.findByPlaceholderText("Send a message...")).toBeInTheDocument();
    await screen.findByRole("button", { name: "Expand folder D:/repo/app" });
    expect(screen.getByText("Start or resume a session")).toBeInTheDocument();
    expect(screen.getByText("Agent (full access)")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Codex" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: "Claude" })).toHaveAttribute(
      "aria-pressed",
      "false",
    );
    expect(screen.getByRole("button", { name: "Gemini" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "OpenCode" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "OpenClaw" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Hermes" })).toBeInTheDocument();
    const folderSelect = screen.getByLabelText("Folder");
    expect(folderSelect).toHaveClass("truncate");
    expect(folderSelect.closest("label")).toHaveClass("sm:max-w-[18rem]");
    expect(folderSelect).toHaveTextContent("repo/app");
    expect(folderSelect).toHaveTextContent("New folder...");
    expect(screen.getByLabelText("Model")).toHaveValue("auto");
    expect(screen.getByLabelText("Reasoning")).toHaveValue("auto");
    expect(screen.getByRole("button", { name: "Start" })).toHaveClass("sm:ml-auto");
  });

  it("creates a new agent session from the empty-state launch composer", async () => {
    renderScreen();

    await screen.findByPlaceholderText("Send a message...");
    await screen.findByRole("button", { name: "Expand folder D:/repo/app" });
    await userEvent.selectOptions(screen.getByLabelText("Folder"), "D:/repo/app");
    await userEvent.click(screen.getByRole("button", { name: "Claude" }));
    await userEvent.click(screen.getByRole("button", { name: "Start" }));

    await waitFor(() =>
      expect(createTerminalSession).toHaveBeenCalledWith({
        kind: "agent",
        platform: "claude",
        command: null,
        title: "claude - D:/repo/app",
        cwd: "D:/repo/app",
        cols: 100,
        rows: 30,
        model: null,
        reasoningEffort: null,
      }),
    );
  });

  it("fills the composer model and reasoning selects from the agent launch catalog", async () => {
    renderScreen();

    await screen.findByPlaceholderText("Send a message...");
    const modelSelect = screen.getByLabelText("Model") as HTMLSelectElement;
    await screen.findByRole("option", { name: "gpt-5.6-sol" });
    expect(Array.from(modelSelect.options).map((option) => option.value)).toEqual([
      "auto",
      "gpt-5.6-sol",
      "gpt-5.6-codex",
    ]);

    const reasoningSelect = screen.getByLabelText("Reasoning") as HTMLSelectElement;
    expect(reasoningSelect).toBeDisabled();

    await userEvent.selectOptions(modelSelect, "gpt-5.6-sol");
    expect(reasoningSelect).toBeEnabled();
    expect(Array.from(reasoningSelect.options).map((option) => option.value)).toEqual([
      "auto",
      "medium",
      "high",
    ]);

    await userEvent.selectOptions(reasoningSelect, "high");
    await userEvent.selectOptions(screen.getByLabelText("Folder"), "D:/repo/app");
    await userEvent.click(screen.getByRole("button", { name: "Start" }));

    await waitFor(() =>
      expect(createTerminalSession).toHaveBeenCalledWith({
        kind: "agent",
        platform: "codex",
        command: null,
        title: "codex - D:/repo/app",
        cwd: "D:/repo/app",
        cols: 100,
        rows: 30,
        model: "gpt-5.6-sol",
        reasoningEffort: "high",
      }),
    );
  });

  it("resets the model selection when the chosen agent does not advertise it", async () => {
    renderScreen();

    await screen.findByPlaceholderText("Send a message...");
    await screen.findByRole("option", { name: "gpt-5.6-sol" });
    await userEvent.selectOptions(screen.getByLabelText("Model"), "gpt-5.6-sol");
    await userEvent.selectOptions(screen.getByLabelText("Reasoning"), "high");

    await userEvent.click(screen.getByRole("button", { name: "Claude" }));

    await waitFor(() => expect(screen.getByLabelText("Model")).toHaveValue("auto"));
    expect(screen.getByLabelText("Reasoning")).toHaveValue("auto");
    expect(screen.getByLabelText("Reasoning")).toBeDisabled();
  });

  it("blocks launching an agent whose CLI is missing and offers the install command", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });

    renderScreen();

    await screen.findByPlaceholderText("Send a message...");
    await userEvent.click(screen.getByRole("button", { name: "Grok" }));

    expect(
      await screen.findByText("Grok CLI is not installed. Install it, then reopen this panel."),
    ).toBeInTheDocument();
    expect(screen.getByText("npm install -g @vibe-kit/grok-cli")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Start" })).toBeDisabled();

    await userEvent.click(screen.getByRole("button", { name: "Copy install command" }));
    expect(writeText).toHaveBeenCalledWith("npm install -g @vibe-kit/grok-cli");
    expect(await screen.findByRole("button", { name: "Copied" })).toBeInTheDocument();

    expect(createTerminalSession).not.toHaveBeenCalled();
  });

  it("surfaces a catalog failure instead of rendering an empty agent panel", async () => {
    vi.mocked(listAgentLaunchOptions).mockRejectedValue(new Error("sidecar offline"));

    renderScreen();

    const notice = await screen.findByTestId("vibe-agent-catalog-error", undefined, {
      timeout: 5000,
    });
    expect(notice).toHaveTextContent("Could not load the agent list: sidecar offline");
    expect(screen.getByRole("button", { name: "Codex" })).toBeInTheDocument();
  });

  it("opens appearance settings and switches Vibe themes from the dialog", async () => {
    renderScreen();

    expect(screen.getByText("Solarized Dark")).toBeInTheDocument();

    await openAppearanceDialog();
    expect(screen.getByRole("button", { name: "Solarized Dark" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByRole("button", { name: "Light" })).toHaveAttribute(
      "aria-pressed",
      "false",
    );

    await userEvent.click(screen.getByRole("button", { name: "Light" }));

    expect(screen.getByRole("button", { name: "Switch Vibe theme" })).toHaveTextContent("Light");
    const lightFolderToggle = screen.getByRole("button", { name: "Expand folder D:/repo/app" });
    expect(lightFolderToggle).toHaveClass("vibe-light-list-trigger");
    expect(lightFolderToggle.parentElement).toHaveClass("vibe-light-group-panel");
    expect(screen.getByText("No terminal tabs yet.").parentElement).toHaveClass("vibe-light-tabbar");
    expect(screen.getByTestId("vibe-tab-strip").parentElement).toHaveClass("vibe-light-workspace");
    expect(screen.getByText("Start or resume a session")).toHaveClass("text-stone-900");

    await userEvent.click(screen.getByRole("button", { name: "Skin" }));

    expect(screen.getByRole("button", { name: "Switch Vibe theme" })).toHaveTextContent("Skin");
    expect(screen.getByLabelText("Vibe skin")).toHaveValue("codex-2007-blue");
    expect(screen.getByText("No terminal tabs yet.").parentElement).toHaveClass("vibe-skin-tabbar");
  });

  it("persists the skin audio toggle from the appearance dialog", async () => {
    const view = renderScreen();

    await openAppearanceDialog();
    const audioToggle = screen.getByLabelText("Skin sound effects");
    expect(audioToggle).toBeChecked();

    await userEvent.click(audioToggle);

    await waitFor(() =>
      expect(window.localStorage.getItem(VIBE_APPEARANCE_STORAGE_KEY)).toContain(
        '"skinAudioEnabled":false',
      ),
    );
    expect(audioToggle).not.toBeChecked();

    view.unmount();
    renderScreen();
    await openAppearanceDialog();

    expect(screen.getByLabelText("Skin sound effects")).not.toBeChecked();
  });

  it("uses cohesive dark colors for the session list and tabs", async () => {
    renderScreen();

    expect(screen.getByText("No terminal tabs yet.").parentElement).toHaveClass("vibe-dark-tabbar");

    const folderToggle = await screen.findByRole("button", {
      name: "Expand folder D:/repo/app",
    });
    expect(folderToggle).toHaveClass("vibe-dark-list-trigger");
    expect(folderToggle.parentElement).toHaveClass("vibe-dark-group-panel");

    await userEvent.click(folderToggle);

    const resumableSession = screen.getByRole("button", { name: /Resume Fix terminal bug/ });
    expect(resumableSession).toHaveClass("vibe-dark-session-card");
    expect(screen.getByText("codex · codex resume s1")).toHaveClass("vibe-dark-session-meta");

    await userEvent.click(resumableSession);

    const tabButton = await screen.findByRole("button", { name: "Fix terminal bug" });
    expect(tabButton).toHaveClass("vibe-tab-trigger");
    expect(tabButton.parentElement).toHaveClass("vibe-dark-tab-active");
    const closeButton = screen.getByRole("button", { name: "Close Fix terminal bug" });
    expect(closeButton).toHaveClass("vibe-dark-tab-close");
    expect(closeButton).not.toHaveClass("rounded-md");
  });

  it("uses cohesive light colors for the session list and tabs", async () => {
    renderScreen();

    await switchThemeFromAppearance("Light");

    expect(screen.getByText("No terminal tabs yet.").parentElement).toHaveClass("vibe-light-tabbar");
    expect(screen.getByTestId("vibe-tab-strip").parentElement).toHaveClass("vibe-light-workspace");

    const folderToggle = await screen.findByRole("button", {
      name: "Expand folder D:/repo/app",
    });
    expect(folderToggle).toHaveClass("vibe-light-list-trigger");
    expect(folderToggle.parentElement).toHaveClass("vibe-light-group-panel");

    await userEvent.click(folderToggle);

    const resumableSession = screen.getByRole("button", { name: /Resume Fix terminal bug/ });
    expect(resumableSession).toHaveClass("vibe-light-session-card");
    expect(screen.getByText("codex · codex resume s1")).toHaveClass("vibe-light-session-meta");

    await userEvent.click(resumableSession);

    const tabButton = await screen.findByRole("button", { name: "Fix terminal bug" });
    expect(tabButton).toHaveClass("vibe-tab-trigger");
    expect(tabButton.parentElement).toHaveClass("vibe-light-tab-active");
    const closeButton = screen.getByRole("button", { name: "Close Fix terminal bug" });
    expect(closeButton).toHaveClass("vibe-light-tab-close");
    expect(closeButton).not.toHaveClass("rounded-md");
  });

  it("renders built-in QQ2007 skin blocks with Chinese decorative UI", async () => {
    renderScreen();

    await switchToSkinTheme();

    expect(screen.getByText("Codex 2007 - 优化 KV 读写成本")).toBeInTheDocument();
    expect(screen.getByText("QQ2007 蓝色经典")).toBeInTheDocument();
    expect(screen.getAllByText("在线").length).toBeGreaterThan(0);
    expect(screen.getAllByText("Codex 小蓝").length).toBeGreaterThan(0);
    expect(screen.getByText("手机在线")).toBeInTheDocument();
    expect(screen.getByText("代码有问题？找我！")).toBeInTheDocument();
    expect(screen.getByText("蓝钻 LV7")).toBeInTheDocument();
    expect(screen.getByText("Codex 好友")).toBeInTheDocument();
    expect(screen.getByText("今天也在陪你写代码。")).toBeInTheDocument();
    expect(screen.getByText("双击发送消息")).toBeInTheDocument();
    expect(screen.getByText("我的好友 (2/8)")).toBeInTheDocument();
    expect(screen.getByTestId("vibe-skin-qq-mascot")).toBeInTheDocument();
    expect(screen.getByTestId("vibe-skin-qq-person")).toBeInTheDocument();
    expect(screen.queryByText("皮肤区域")).not.toBeInTheDocument();
    expect(screen.getByText("Codex 已连接")).toBeInTheDocument();
    expect(screen.getByText("QQ2007 皮肤模式")).toBeInTheDocument();

    const controls = screen.getByTestId("vibe-window-controls");
    expect(controls).toHaveAttribute("aria-hidden", "true");
    expect(controls).toHaveTextContent("—");
    expect(controls).toHaveTextContent("□");
    expect(controls).toHaveTextContent("×");
    expect(
      screen.queryByRole("button", { name: /minimize|maximize|close window/i }),
    ).not.toBeInTheDocument();
  });

  it("renders the built-in rescue pups skin with themed decorative regions", async () => {
    renderScreen();

    await switchToSkinTheme();
    await openAppearanceDialog();
    await userEvent.selectOptions(screen.getByLabelText("Vibe skin"), "rescue-pups-adventure-bay");

    expect(screen.getByLabelText("Vibe skin")).toHaveValue("rescue-pups-adventure-bay");
    expect(screen.getByText("汪汪队终端救援站")).toBeInTheDocument();
    expect(screen.getByText("冒险湾主题")).toBeInTheDocument();
    expect(screen.getByText("救援待命")).toBeInTheDocument();
    expect(screen.getAllByText("莱德队长").length).toBeGreaterThan(0);
    expect(screen.getByText("总部在线")).toBeInTheDocument();
    expect(screen.getByText("没有困难的任务，只有勇敢的队员。")).toBeInTheDocument();
    expect(screen.getAllByText("汪汪队总部").length).toBeGreaterThan(0);
    expect(screen.getByText("狗狗们已在总部集结，随时支援终端任务。")).toBeInTheDocument();
    expect(screen.getByText("莱德队长正在调度狗狗们")).toBeInTheDocument();
    expect(screen.getByText("汪汪队员")).toBeInTheDocument();
    expect(screen.getByText("狗狗们")).toBeInTheDocument();
    expect(screen.getByText("冒险湾市政")).toBeInTheDocument();
    expect(screen.getByText("古微市长")).toBeInTheDocument();
    expect(screen.getByText("咕咕鸡")).toBeInTheDocument();
    expect(screen.getByText("冒险湾已连接")).toBeInTheDocument();
    expect(screen.getByText("救援队待命")).toBeInTheDocument();
    expect(screen.getByText("出动")).toBeInTheDocument();
    expect(screen.getByTestId("vibe-skin-rescue-avatar")).toBeInTheDocument();
    expect(screen.getByTestId("vibe-skin-rescue-hq")).toBeInTheDocument();
    expect(screen.getByTestId("vibe-skin-rescue-dogs")).toBeInTheDocument();
    expect(screen.getByTestId("vibe-skin-rescue-mayor")).toBeInTheDocument();
    expect(screen.getByTestId("vibe-skin-rescue-chicken")).toBeInTheDocument();
    expect(screen.queryByText("皮肤区域")).not.toBeInTheDocument();
  });

  it("renders the built-in starship cockpit skin with Chinese HUD blocks", async () => {
    renderScreen();

    await switchToSkinTheme();
    await openAppearanceDialog();
    await userEvent.selectOptions(screen.getByLabelText("Vibe skin"), "starship-cockpit");

    expect(screen.getByLabelText("Vibe skin")).toHaveValue("starship-cockpit");
    expect(screen.getByText("星舰驾驶舱 - Vibe 终端")).toBeInTheDocument();
    expect(screen.getByText("深空跃迁 / 指令甲板")).toBeInTheDocument();
    expect(screen.getByText("舰桥 AI 核心")).toBeInTheDocument();
    expect(screen.getByText("量子链路在线")).toBeInTheDocument();
    expect(screen.queryByText("星舰主视窗")).not.toBeInTheDocument();
    expect(screen.queryByText("舰体姿态、雷达扫描与遥测输出已同步到 Vibe 工作区。")).not.toBeInTheDocument();
    expect(screen.queryByText("雷达扫描")).not.toBeInTheDocument();
    expect(screen.queryByText("舰体模拟")).not.toBeInTheDocument();
    expect(screen.queryByText("全息结构图")).not.toBeInTheDocument();
    expect(screen.queryByText("姿态慢速旋转")).not.toBeInTheDocument();
    expect(screen.getByText("近轨目标追踪")).toBeInTheDocument();
    const starmapTitle = screen.getByText("航线星图");
    const telemetryTitle = screen.getByText("遥测输出");
    expect(starmapTitle.compareDocumentPosition(telemetryTitle)).toBe(
      Node.DOCUMENT_POSITION_FOLLOWING,
    );
    expect(screen.getByText("舰桥链路已建立")).toBeInTheDocument();
    expect(screen.getByText("深空航行模式")).toBeInTheDocument();
    expect(screen.getByText("舰桥")).toBeInTheDocument();
    expect(screen.getByText("出发下一个星球")).toBeInTheDocument();
    expect(screen.getByText("选择智能体、航线与推理功率，舰桥将打开新的终端任务。")).toBeInTheDocument();
    expect(screen.getByText("武器选项")).toBeInTheDocument();
    expect(screen.getByText("全舰权限")).toBeInTheDocument();
    expect(screen.getByText("任务模式")).toBeInTheDocument();
    expect(screen.getByText("深空探索")).toBeInTheDocument();
    expect(screen.getByPlaceholderText("输入本次航行指令...")).toBeInTheDocument();
    expect(screen.getByLabelText("航线目录")).toBeInTheDocument();
    expect(screen.getByLabelText("舰载模型")).toBeInTheDocument();
    expect(screen.getByLabelText("推理功率")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "跃迁启动" })).toBeInTheDocument();
    expect(screen.getByTestId("vibe-skin-space-ai-core")).toBeInTheDocument();
    expect(screen.getAllByTestId("vibe-skin-space-ship").length).toBeGreaterThanOrEqual(1);
    expect(screen.getByTestId("vibe-skin-space-radar")).toBeInTheDocument();
    expect(screen.getByTestId("vibe-skin-space-telemetry")).toBeInTheDocument();
    expect(screen.getByTestId("vibe-skin-space-starmap")).toBeInTheDocument();
    expect(screen.getByTestId("vibe-skin-space-planets")).toHaveAttribute("aria-hidden", "true");
    const planets = screen.getAllByTestId("vibe-skin-space-planet");
    expect(planets).toHaveLength(3);
    expect(planets[0]).toHaveClass("vibe-skin-space-planet-large");
    expect(planets[1]).toHaveClass("vibe-skin-space-planet-medium");
    expect(planets[2]).toHaveClass("vibe-skin-space-planet-small");
    expect(document.querySelector(".vibe-skin--starship-cockpit")).toBeTruthy();
    expect(document.querySelector(".vibe-skin-space-card")).toBeTruthy();
    expect(document.querySelector(".vibe-skin-space-telemetry-card")).toBeTruthy();
    expect(document.querySelector(".vibe-skin-right-rail")).toHaveClass(
      "overflow-y-auto",
      "vibe-scrollbar-skin",
    );
    expect(document.querySelector(".vibe-skin-showcase-stage")).toBeFalsy();
    expect(screen.queryByText("皮肤区域")).not.toBeInTheDocument();
  });

  it("persists the selected Vibe skin across remounts", async () => {
    const view = renderScreen();

    await switchToSkinTheme();
    await openAppearanceDialog();
    await userEvent.selectOptions(screen.getByLabelText("Vibe skin"), "starship-cockpit");

    await waitFor(() =>
      expect(window.localStorage.getItem(VIBE_APPEARANCE_STORAGE_KEY)).toContain(
        "starship-cockpit",
      ),
    );

    view.unmount();
    renderScreen();

    expect(await screen.findByRole("button", { name: "Switch Vibe theme" })).toHaveTextContent(
      "Skin",
    );
    expect(await screen.findByText("星舰驾驶舱 - Vibe 终端")).toBeInTheDocument();

    await openAppearanceDialog();
    expect(screen.getByLabelText("Vibe skin")).toHaveValue("starship-cockpit");
  });

  it("renders custom rescue-style decorations from a stored skin package manifest", async () => {
    window.localStorage.setItem(
      VIBE_SKIN_STORAGE_KEY,
      JSON.stringify({
        id: "uploaded-rescue",
        name: "Uploaded Rescue",
        ui: {
          accent: "#0b7fec",
          background: "#78d4ff",
          panel: "rgba(255, 250, 229, 0.9)",
          panelStrong: "rgba(255, 255, 255, 0.96)",
          panelSubtle: "rgba(191, 230, 255, 0.82)",
          border: "rgba(8, 93, 174, 0.42)",
          text: "#102a43",
          mutedText: "#43627d",
          button: "#0b7fec",
          buttonText: "#ffffff",
          buttonHover: "#1687ef",
        },
        blocks: {
          titlebar: {
            title: "上传救援主题",
            subtitle: "皮肤包",
            badge: "待命",
          },
          profile: {
            name: "莱德队长",
            status: "总部在线",
            signature: "自定义包",
            badge: "队长",
          },
          showcase: {
            enabled: true,
            title: "汪汪队总部",
            subtitle: "上传包",
            body: "来自皮肤文件包。",
            badge: "救援总部",
            footer: "自定义展示",
          },
        },
        decorations: {
          variant: "rescue-pups",
          titlebarMark: "汪",
          avatarTemplate: "rescue-rider",
          showcaseTemplate: "rescue-hq",
          rightCards: [
            {
              template: "rescue-dog-team",
              title: "上传狗狗队",
              badge: "狗狗们",
              items: [{ label: "红色救援狗狗", tone: "red" }],
            },
            {
              template: "rescue-civic",
              title: "上传市政",
              items: [
                { label: "古微市长", template: "rescue-mayor" },
                { label: "咕咕鸡", template: "rescue-chicken" },
              ],
            },
          ],
        },
      }),
    );
    renderScreen();

    await switchToSkinTheme();
    await openAppearanceDialog();

    expect(screen.getByLabelText("Vibe skin")).toHaveValue("uploaded-rescue");
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));

    expect(document.querySelector(".vibe-skin--rescue-pups")).toBeTruthy();
    expect(screen.getByText("上传救援主题")).toBeInTheDocument();
    expect(screen.getByText("皮肤包")).toBeInTheDocument();
    expect(screen.getByText("来自皮肤文件包。")).toBeInTheDocument();
    expect(screen.getByText("上传狗狗队")).toBeInTheDocument();
    expect(screen.getByText("上传市政")).toBeInTheDocument();
    expect(screen.getByTestId("vibe-skin-rescue-avatar")).toBeInTheDocument();
    expect(screen.getByTestId("vibe-skin-rescue-hq")).toBeInTheDocument();
    expect(screen.getByTestId("vibe-skin-rescue-dogs")).toBeInTheDocument();
    expect(screen.getByTestId("vibe-skin-rescue-mayor")).toBeInTheDocument();
    expect(screen.getByTestId("vibe-skin-rescue-chicken")).toBeInTheDocument();
  });

  it("does not render QQ2007 decorative skin blocks in dark or light themes", async () => {
    renderScreen();

    expect(
      await screen.findByRole("button", { name: "Expand folder D:/repo/app" }),
    ).toBeInTheDocument();
    expect(screen.queryByText("Codex 好友")).not.toBeInTheDocument();
    expect(screen.queryByTestId("vibe-window-controls")).not.toBeInTheDocument();

    await switchThemeFromAppearance("Light");

    expect(screen.queryByText("Codex 好友")).not.toBeInTheDocument();
    expect(screen.queryByTestId("vibe-window-controls")).not.toBeInTheDocument();
  });

  it("renders custom skin showcase regions", async () => {
    window.localStorage.setItem(
      VIBE_SKIN_STORAGE_KEY,
      JSON.stringify({
        id: "showcase-skin",
        name: "Showcase Skin",
        ui: {
          accent: "#00ffee",
          background: "#001018",
          panel: "rgba(2,28,40,0.78)",
          panelStrong: "rgba(4,42,58,0.92)",
          panelSubtle: "rgba(0,255,238,0.12)",
          border: "rgba(0,255,238,0.35)",
          text: "#f4fbff",
          mutedText: "#9be7ff",
          button: "#00ffee",
          buttonText: "#001018",
          buttonHover: "#54fff5",
        },
        regions: {
          rightRail: { background: "#123456" },
          terminalShell: { background: "#010203" },
          showcaseStage: { background: "#102030" },
        },
        blocks: {
          titlebar: {
            title: "霓虹终端",
            subtitle: "自定义标题栏",
            badge: "自定义皮肤",
          },
          profile: {
            name: "霓虹用户",
            status: "忙碌",
            signature: "正在调试右侧展示区",
            badge: "VIP",
            avatar: "data:image/png;base64,AAAA",
          },
          showcase: {
            title: "右侧QQ秀",
            subtitle: "Neon Figure",
            body: "blocks.showcase 控制展示内容。",
            badge: "Custom Rail",
            figure: "data:image/png;base64,BBBB",
            footer: "region keys",
          },
          statusbar: {
            left: "霓虹已连接",
            right: "状态栏右侧",
          },
        },
      }),
    );
    renderScreen();

    await switchToSkinTheme();

    await openAppearanceDialog();
    expect(screen.getByLabelText("Vibe skin")).toHaveValue("showcase-skin");
    expect(screen.getByText("霓虹终端")).toBeInTheDocument();
    expect(screen.getByText("霓虹用户")).toBeInTheDocument();
    expect(screen.getByText("忙碌")).toBeInTheDocument();
    expect(screen.getByText("右侧QQ秀")).toBeInTheDocument();
    expect(screen.getByText("Custom Rail")).toBeInTheDocument();
    expect(screen.getByText("terminalShell")).toBeInTheDocument();
    expect(screen.getByText("rightRail")).toBeInTheDocument();
    expect(screen.getByText("showcaseStage")).toBeInTheDocument();
    expect(screen.getByText("region keys")).toBeInTheDocument();
    expect(screen.getByRole("img", { name: "霓虹用户 avatar" })).toHaveAttribute(
      "src",
      "data:image/png;base64,AAAA",
    );
    expect(screen.getByRole("img", { name: "右侧QQ秀 figure" })).toHaveAttribute(
      "src",
      "data:image/png;base64,BBBB",
    );
  });

  it("renders legacy showcase content when blocks.showcase is absent", async () => {
    window.localStorage.setItem(
      VIBE_SKIN_STORAGE_KEY,
      JSON.stringify({
        id: "legacy-showcase-skin",
        name: "Legacy Showcase Skin",
        ui: {
          accent: "#1678d8",
          background: "#dff5ff",
          panel: "rgba(232,247,255,0.78)",
          panelStrong: "rgba(255,255,255,0.92)",
          panelSubtle: "rgba(216,239,255,0.68)",
          border: "rgba(15,99,184,0.34)",
          text: "#0d315d",
          mutedText: "#386b9e",
          button: "#1678d8",
          buttonText: "#ffffff",
          buttonHover: "#0f61ae",
        },
        showcase: {
          enabled: true,
          title: "旧版展示标题",
          badge: "旧版徽标",
          image: "data:image/png;base64,CCCC",
          footer: "旧版页脚",
        },
      }),
    );
    renderScreen();

    await switchToSkinTheme();

    expect(screen.getByText("旧版展示标题")).toBeInTheDocument();
    expect(screen.getByText("旧版徽标")).toBeInTheDocument();
    expect(screen.getByRole("img", { name: "旧版展示标题 figure" })).toHaveAttribute(
      "src",
      "data:image/png;base64,CCCC",
    );
    expect(screen.getByText("旧版页脚")).toBeInTheDocument();
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

    await openAppearanceDialog();
    await userEvent.upload(screen.getByLabelText("Choose Vibe skin package"), skinFile);

    await waitFor(() => expect(screen.getByLabelText("Vibe skin")).toHaveValue("custom-neon"));
    expect(screen.getByRole("button", { name: "Switch Vibe theme" })).toHaveTextContent("Skin");
    expect(window.localStorage.getItem(VIBE_SKIN_STORAGE_KEY)).toContain("Custom Neon");

    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await expandProjectDirectory();
    await userEvent.click(await screen.findByRole("button", { name: /Resume Fix terminal bug/ }));

    expect(await screen.findByTestId("terminal-pane-term-1")).toHaveAttribute(
      "data-theme-override",
      "yes",
    );
    expect(await screen.findByTestId("terminal-pane-term-1")).toHaveAttribute(
      "data-transparent-surface",
      "yes",
    );
  });

  it("renders the skin taskbar and replaces the old skin status bar when enabled", async () => {
    renderScreen();

    await switchToSkinTheme();

    expect(screen.getByRole("button", { name: "开始" })).toBeInTheDocument();
    expect(screen.getAllByText("AI Switch 终端").length).toBeGreaterThan(0);
    expect(screen.getByText("Codex 已连接")).toBeInTheDocument();
    expect(screen.getByText("QQ2007 皮肤模式")).toBeInTheDocument();
    expect(screen.getByText("Vibe")).toBeInTheDocument();
    expect(screen.getAllByText("在线").length).toBeGreaterThan(0);
    expect(document.querySelector(".vibe-skin-taskbar")).toBeTruthy();
    expect(document.querySelector(".vibe-skin-status-bar")).toBeFalsy();
  });

  it("falls back to the skin status bar when a custom skin disables the taskbar", async () => {
    window.localStorage.setItem(
      VIBE_SKIN_STORAGE_KEY,
      JSON.stringify({
        id: "no-taskbar",
        name: "No Taskbar",
        ui: {
          accent: "#1678d8",
          background: "#dff5ff",
          panel: "rgba(232,247,255,0.78)",
          panelStrong: "rgba(255,255,255,0.92)",
          panelSubtle: "rgba(216,239,255,0.68)",
          border: "rgba(15,99,184,0.34)",
          text: "#0d315d",
          mutedText: "#386b9e",
          button: "#1678d8",
          buttonText: "#ffffff",
          buttonHover: "#0f61ae",
        },
        blocks: {
          taskbar: {
            enabled: false,
          },
          statusbar: {
            left: "自定义左侧状态",
            right: "自定义右侧状态",
          },
        },
      }),
    );
    renderScreen();

    await switchToSkinTheme();

    expect(screen.queryByRole("button", { name: "开始" })).not.toBeInTheDocument();
    expect(document.querySelector(".vibe-skin-taskbar")).toBeFalsy();
    expect(document.querySelector(".vibe-skin-status-bar")).toBeTruthy();
    expect(screen.getByText("自定义左侧状态")).toBeInTheDocument();
    expect(screen.getByText("自定义右侧状态")).toBeInTheDocument();
  });

  it("opens the taskbar start menu and executes only safe actions", async () => {
    window.localStorage.setItem(
      VIBE_SKIN_STORAGE_KEY,
      JSON.stringify({
        id: "menu-skin",
        name: "Menu Skin",
        ui: {
          accent: "#1678d8",
          background: "#dff5ff",
          panel: "rgba(232,247,255,0.78)",
          panelStrong: "rgba(255,255,255,0.92)",
          panelSubtle: "rgba(216,239,255,0.68)",
          border: "rgba(15,99,184,0.34)",
          text: "#0d315d",
          mutedText: "#386b9e",
          button: "#1678d8",
          buttonText: "#ffffff",
          buttonHover: "#0f61ae",
        },
        blocks: {
          taskbar: {
            startMenu: {
              items: [
                { label: "外观设置", action: "openAppearance" },
                { label: "切换暗色主题", action: "setTheme", theme: "dark" },
                { label: "非法动作", action: "nativeCloseWindow" },
                { type: "separator" },
                { label: "不可点击", disabled: true },
              ],
            },
          },
        },
      }),
    );
    renderScreen();

    await switchToSkinTheme();
    await userEvent.click(screen.getByRole("button", { name: "开始" }));

    expect(screen.getByRole("menu", { name: "开始菜单" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "外观设置" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "切换暗色主题" })).toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: "非法动作" })).not.toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "不可点击" })).toBeDisabled();

    await userEvent.click(screen.getByRole("menuitem", { name: "切换暗色主题" }));

    expect(screen.queryByRole("button", { name: "开始" })).not.toBeInTheDocument();
    expect(screen.getByText("Solarized Dark")).toBeInTheDocument();

    await switchToSkinTheme();
    await userEvent.click(screen.getByRole("button", { name: "开始" }));
    await userEvent.click(screen.getByRole("menuitem", { name: "外观设置" }));

    expect(await screen.findByRole("dialog", { name: "Appearance" })).toBeInTheDocument();
    expect(screen.queryByRole("menu", { name: "开始菜单" })).not.toBeInTheDocument();
  });

  it("keeps skin select, import, and clear controls inside the appearance dialog", async () => {
    window.localStorage.setItem(
      VIBE_SKIN_STORAGE_KEY,
      JSON.stringify({
        id: "stored-popup-skin",
        name: "Stored Popup Skin",
        ui: {
          accent: "#1678d8",
          background: "#dff5ff",
          panel: "rgba(232,247,255,0.78)",
          panelStrong: "rgba(255,255,255,0.92)",
          panelSubtle: "rgba(216,239,255,0.68)",
          border: "rgba(15,99,184,0.34)",
          text: "#0d315d",
          mutedText: "#386b9e",
          button: "#1678d8",
          buttonText: "#ffffff",
          buttonHover: "#0f61ae",
        },
      }),
    );
    renderScreen();

    expect(screen.queryByLabelText("Vibe skin")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Import skin" })).not.toBeInTheDocument();

    await openAppearanceDialog();

    const dialog = screen.getByRole("dialog", { name: "Appearance" });
    expect(dialog).toContainElement(screen.getByLabelText("Vibe skin"));
    expect(dialog).toContainElement(screen.getByRole("button", { name: "Import skin" }));
    expect(dialog).toContainElement(screen.getByRole("button", { name: "Clear custom skin" }));

    await userEvent.selectOptions(screen.getByLabelText("Vibe skin"), "codex-2007-blue");
    expect(screen.getByLabelText("Vibe skin")).toHaveValue("codex-2007-blue");

    await userEvent.click(screen.getByRole("button", { name: "Clear custom skin" }));
    expect(window.localStorage.getItem(VIBE_SKIN_STORAGE_KEY)).toBeNull();
  });

  it("creates a new agent session through the modal", async () => {
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "New session" }));
    await screen.findByRole("dialog", { name: "Create session" });
    const existingFolderSelect = screen.getByLabelText("Existing folder") as HTMLSelectElement;
    const existingFolderOption = Array.from(existingFolderSelect.options).find(
      (option) => option.value === "D:/repo/app",
    );
    expect(existingFolderOption).toHaveTextContent("repo/app");
    expect(existingFolderOption).not.toHaveTextContent("D:/repo/app");
    await userEvent.selectOptions(existingFolderSelect, "D:/repo/app");
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
        model: null,
        reasoningEffort: null,
      }),
    );
  });
});
