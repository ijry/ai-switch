import { render, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  normalizeClaudeLightDiffOutput,
  XtermPane,
} from "../../src/components/terminal/XtermPane";
import type { TerminalSession } from "../../src/lib/api/types";

const terminalConstructorOptions = vi.hoisted(() => [] as Array<Record<string, unknown>>);
const terminalInstances = vi.hoisted(
  () =>
    [] as Array<{
      write: ReturnType<typeof vi.fn>;
      parser: {
        registerCsiHandler: ReturnType<typeof vi.fn>;
      };
    }>,
);
const outputListeners = new Map<string, (payload: unknown) => void>();
const subscribe = vi.fn(async (eventName: string, listener: (payload: unknown) => void) => {
  outputListeners.set(eventName, listener);
  return () => {
    outputListeners.delete(eventName);
  };
});

vi.mock("../../src/lib/transport", () => ({
  getTransport: () => ({
    subscribe,
  }),
}));

vi.mock("../../src/lib/api/client", () => ({
  resizeTerminal: vi.fn(async () => undefined),
  writeTerminalInput: vi.fn(async () => undefined),
}));

vi.mock("@xterm/addon-fit", () => ({
  FitAddon: class {
    fit = vi.fn();
  },
}));

vi.mock("@xterm/xterm", () => ({
  Terminal: class {
    cols = 80;
    rows = 24;
    options: Record<string, unknown>;
    dispose = vi.fn();
    focus = vi.fn();
    loadAddon = vi.fn();
    open = vi.fn();
    refresh = vi.fn();
    write = vi.fn();
    writeln = vi.fn();
    parser = {
      registerCsiHandler: vi.fn(() => ({ dispose: vi.fn() })),
    };
    onData = vi.fn(() => ({ dispose: vi.fn() }));

    constructor(options: Record<string, unknown>) {
      this.options = options;
      terminalConstructorOptions.push(options);
      terminalInstances.push(this);
    }
  },
}));

const session: TerminalSession = {
  id: "terminal-1",
  title: "Codex",
  platform: "codex",
  cwd: "D:/Repos/app",
  command: "codex",
  status: "running",
  createdAt: 123,
};

describe("XtermPane", () => {
  afterEach(() => {
    subscribe.mockClear();
    terminalConstructorOptions.length = 0;
    terminalInstances.length = 0;
    outputListeners.clear();
  });

  it("changes Claude's dark diff palette to readable light diff colours", () => {
    const output =
      "\u001b[38;2;255;255;255;48;2;68;20;24m- removed\u001b[0m " +
      "\u001b[38;5;15;48;5;22m+ added\u001b[0m " +
      "\u001b[31mordinary error\u001b[0m \u001b[32mordinary success\u001b[0m";

    expect(normalizeClaudeLightDiffOutput(output)).toBe(
      "\u001b[38;2;153;27;27;48;2;254;226;226m- removed\u001b[0m " +
        "\u001b[38;2;22;101;52;48;2;220;252;231m+ added\u001b[0m " +
        "\u001b[31mordinary error\u001b[0m \u001b[32mordinary success\u001b[0m",
    );
  });

  it("applies the light diff palette only to Claude light Vibe output", async () => {
    const claudeSession: TerminalSession = {
      ...session,
      id: "claude-terminal",
      platform: "claude",
      title: "Claude",
    };
    render(<XtermPane session={claudeSession} themeMode="light" />);

    await waitFor(() => expect(terminalInstances).toHaveLength(1));
    outputListeners.get("terminal://output")?.({
      sessionId: claudeSession.id,
      data: "\u001b[48;2;18;54;30m+ added\u001b[0m",
    });

    expect(terminalInstances[0]?.write).toHaveBeenCalledWith(
      "\u001b[38;2;22;101;52;48;2;220;252;231m+ added\u001b[0m",
    );
  });

  it("does not change Codex output in a light Vibe pane", async () => {
    const codexSession: TerminalSession = {
      ...session,
      id: "codex-light-terminal",
    };
    render(<XtermPane session={codexSession} themeMode="light" />);

    await waitFor(() => expect(terminalInstances).toHaveLength(1));
    outputListeners.get("terminal://output")?.({
      sessionId: codexSession.id,
      data: "\u001b[48;2;18;54;30m+ added\u001b[0m",
    });

    expect(terminalInstances[0]?.write).toHaveBeenCalledWith(
      "\u001b[48;2;18;54;30m+ added\u001b[0m",
    );
  });

  it("updates Claude diff conversion when the Vibe theme changes without rebuilding xterm", async () => {
    const claudeSession: TerminalSession = {
      ...session,
      id: "claude-theme-terminal",
      platform: "claude",
      title: "Claude",
    };
    const { rerender } = render(<XtermPane session={claudeSession} themeMode="dark" />);

    await waitFor(() => expect(terminalInstances).toHaveLength(1));
    outputListeners.get("terminal://output")?.({
      sessionId: claudeSession.id,
      data: "\u001b[48;5;88m- removed\u001b[0m",
    });
    expect(terminalInstances[0]?.write).toHaveBeenLastCalledWith(
      "\u001b[48;5;88m- removed\u001b[0m",
    );

    rerender(<XtermPane session={claudeSession} themeMode="light" />);
    outputListeners.get("terminal://output")?.({
      sessionId: claudeSession.id,
      data: "\u001b[48;5;88m- removed\u001b[0m",
    });

    expect(terminalInstances).toHaveLength(1);
    expect(terminalInstances[0]?.write).toHaveBeenLastCalledWith(
      "\u001b[38;2;153;27;27;48;2;254;226;226m- removed\u001b[0m",
    );
  });

  it("keeps Claude diff colours unchanged for transparent skin themes", async () => {
    const claudeSession: TerminalSession = {
      ...session,
      id: "claude-skin-terminal",
      platform: "claude",
      title: "Claude",
    };
    render(<XtermPane session={claudeSession} themeMode="light" transparentSurface />);

    await waitFor(() => expect(terminalInstances).toHaveLength(1));
    outputListeners.get("terminal://output")?.({
      sessionId: claudeSession.id,
      data: "\u001b[48;2;134;40;48m- removed\u001b[0m",
    });

    expect(terminalInstances[0]?.write).toHaveBeenCalledWith(
      "\u001b[48;2;134;40;48m- removed\u001b[0m",
    );
  });

  it("normalizes a Claude diff SGR sequence split across terminal output events", async () => {
    const claudeSession: TerminalSession = {
      ...session,
      id: "claude-split-terminal",
      platform: "claude",
      title: "Claude",
    };
    render(<XtermPane session={claudeSession} themeMode="light" />);

    await waitFor(() => expect(terminalInstances).toHaveLength(1));
    outputListeners.get("terminal://output")?.({
      sessionId: claudeSession.id,
      data: "\u001b[48;2;18;",
    });
    expect(terminalInstances[0]?.write).not.toHaveBeenCalled();

    outputListeners.get("terminal://output")?.({
      sessionId: claudeSession.id,
      data: "54;30m+ added\u001b[0m",
    });
    expect(terminalInstances[0]?.write).toHaveBeenCalledWith(
      "\u001b[38;2;22;101;52;48;2;220;252;231m+ added\u001b[0m",
    );
  });

  it("subscribes to terminal events through the active transport", async () => {
    const { container } = render(<XtermPane session={session} />);

    await waitFor(() => expect(subscribe).toHaveBeenCalledTimes(3));
    const eventNames = subscribe.mock.calls.map((call: unknown[]) => call[0]);
    expect(eventNames).toEqual([
      "terminal://output",
      "terminal://exit",
      "terminal://error",
    ]);
    expect(container.querySelector(".xterm-pane-scrollbar-dark")).not.toBeNull();
  });

  it("marks skin panes transparent and uses a transparent xterm background", async () => {
    const { container } = render(
      <XtermPane
        session={session}
        themeMode="light"
        themeOverride={{
          background: "#010203",
          foreground: "#eafcff",
        }}
        transparentSurface
      />,
    );

    expect(container.querySelector(".xterm-pane-skin-transparent")).not.toBeNull();
    expect(container.querySelector(".xterm-pane-scrollbar-skin")).not.toBeNull();
    await waitFor(() => expect(terminalConstructorOptions).toHaveLength(1));

    expect(terminalConstructorOptions[0]?.allowTransparency).toBe(true);
    expect(terminalConstructorOptions[0]?.theme).toMatchObject({
      background: "transparent",
      foreground: "#eafcff",
    });
  });

  it("marks light panes with the light scrollbar theme", () => {
    const { container } = render(<XtermPane session={session} themeMode="light" />);

    expect(container.querySelector(".xterm-pane-scrollbar-light")).not.toBeNull();
  });

  it("thins the xterm slider through the overview ruler width and themes its colours", async () => {
    render(<XtermPane session={session} />);

    await waitFor(() => expect(terminalConstructorOptions).toHaveLength(1));

    expect(terminalConstructorOptions[0]?.overviewRuler).toEqual({
      showBottomBorder: false,
      showTopBorder: false,
      width: 8,
    });
    expect(terminalConstructorOptions[0]?.theme).toMatchObject({
      overviewRulerBorder: "rgba(0, 0, 0, 0)",
      scrollbarSliderActiveBackground: "rgba(147, 161, 161, 0.68)",
      scrollbarSliderBackground: "rgba(147, 161, 161, 0.42)",
      scrollbarSliderHoverBackground: "rgba(147, 161, 161, 0.55)",
    });
  });

  it("keeps long agent output and preserves content erased by terminal redraws", async () => {
    render(<XtermPane session={session} />);

    await waitFor(() => expect(terminalConstructorOptions).toHaveLength(1));

    expect(terminalConstructorOptions[0]?.scrollback).toBe(10_000);
    expect(terminalConstructorOptions[0]?.scrollOnEraseInDisplay).toBe(true);
  });

  it("protects Claude scrollback from buffer switches and scrollback clears", async () => {
    const claudeSession: TerminalSession = {
      ...session,
      id: "claude-scrollback-terminal",
      platform: "claude",
      title: "Claude",
    };
    render(<XtermPane session={claudeSession} />);

    await waitFor(() => expect(terminalInstances).toHaveLength(1));

    const handlers = terminalInstances[0]?.parser.registerCsiHandler.mock.calls as Array<
      [{ final: string; prefix?: string }, (params: number[]) => boolean]
    >;
    const privateModeHandler = handlers.find(
      ([identifier]) => identifier.prefix === "?" && identifier.final === "h",
    )?.[1];
    const eraseDisplayHandler = handlers.find(
      ([identifier]) => !identifier.prefix && identifier.final === "J",
    )?.[1];

    expect(privateModeHandler?.([1049])).toBe(true);
    expect(privateModeHandler?.([1047])).toBe(true);
    expect(privateModeHandler?.([47])).toBe(true);
    expect(privateModeHandler?.([25])).toBe(false);
    expect(eraseDisplayHandler?.([3])).toBe(true);
    expect(eraseDisplayHandler?.([2])).toBe(false);
  });

  it("does not install Claude scrollback guards for other agents", async () => {
    render(<XtermPane session={session} />);

    await waitFor(() => expect(terminalInstances).toHaveLength(1));

    expect(terminalInstances[0]?.parser.registerCsiHandler).not.toHaveBeenCalled();
  });
});
