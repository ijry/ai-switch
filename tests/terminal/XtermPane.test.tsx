import { render, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { XtermPane } from "../../src/components/terminal/XtermPane";
import type { TerminalSession } from "../../src/lib/api/types";

const subscribe = vi.fn(async () => vi.fn());
const terminalConstructorOptions = vi.hoisted(() => [] as Array<Record<string, unknown>>);

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
    onData = vi.fn(() => ({ dispose: vi.fn() }));

    constructor(options: Record<string, unknown>) {
      this.options = options;
      terminalConstructorOptions.push(options);
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
  });

  it("subscribes to terminal events through the active transport", async () => {
    render(<XtermPane session={session} />);

    await waitFor(() => expect(subscribe).toHaveBeenCalledTimes(3));
    const eventNames = subscribe.mock.calls.map((call: unknown[]) => call[0]);
    expect(eventNames).toEqual([
      "terminal://output",
      "terminal://exit",
      "terminal://error",
    ]);
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
    await waitFor(() => expect(terminalConstructorOptions).toHaveLength(1));

    expect(terminalConstructorOptions[0]?.allowTransparency).toBe(true);
    expect(terminalConstructorOptions[0]?.theme).toMatchObject({
      background: "transparent",
      foreground: "#eafcff",
    });
  });
});
