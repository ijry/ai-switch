import { render, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { XtermPane } from "../../src/components/terminal/XtermPane";
import type { TerminalSession } from "../../src/lib/api/types";

const subscribe = vi.fn(async () => vi.fn());

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
    options: Record<string, unknown> = {};
    dispose = vi.fn();
    focus = vi.fn();
    loadAddon = vi.fn();
    open = vi.fn();
    refresh = vi.fn();
    write = vi.fn();
    writeln = vi.fn();
    onData = vi.fn(() => ({ dispose: vi.fn() }));
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
});
