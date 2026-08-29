import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import { useEffect, useMemo, useRef } from "react";
import { resizeTerminal, writeTerminalInput } from "../../lib/api/client";
import type {
  TerminalErrorEvent,
  TerminalExitEvent,
  TerminalOutputEvent,
  TerminalSession,
  TerminalStatus,
} from "../../lib/api/types";
import { getTransport } from "../../lib/transport";
import type { VibeTerminalTheme } from "../../lib/vibeSkin";

type XtermPaneProps = {
  session: TerminalSession;
  active?: boolean;
  themeMode?: "dark" | "light";
  themeOverride?: VibeTerminalTheme;
  transparentSurface?: boolean;
  onStatusChange?: (sessionId: string, status: TerminalStatus, exitCode?: number | null) => void;
};

type ClaudeDiffTone = "added" | "removed";

const CLAUDE_TRUECOLOR_DIFF_BACKGROUNDS = new Map<string, ClaudeDiffTone>([
  ["68;20;24", "removed"],
  ["134;40;48", "removed"],
  ["18;54;30", "added"],
  ["30;104;52", "added"],
]);

const CLAUDE_256_DIFF_BACKGROUNDS = new Map<string, ClaudeDiffTone>([
  ["52", "removed"],
  ["88", "removed"],
  ["22", "added"],
  ["28", "added"],
]);

const CLAUDE_LIGHT_DIFF_COLORS: Record<
  ClaudeDiffTone,
  { background: string[]; foreground: string[] }
> = {
  added: {
    background: ["48", "2", "220", "252", "231"],
    foreground: ["38", "2", "22", "101", "52"],
  },
  removed: {
    background: ["48", "2", "254", "226", "226"],
    foreground: ["38", "2", "153", "27", "27"],
  },
};

function findClaudeDiffTone(parameters: string[]): ClaudeDiffTone | null {
  for (let index = 0; index < parameters.length; index += 1) {
    if (parameters[index] !== "48") {
      continue;
    }
    if (parameters[index + 1] === "2") {
      const color = parameters.slice(index + 2, index + 5).join(";");
      const tone = CLAUDE_TRUECOLOR_DIFF_BACKGROUNDS.get(color);
      if (tone) {
        return tone;
      }
    } else if (parameters[index + 1] === "5") {
      const tone = CLAUDE_256_DIFF_BACKGROUNDS.get(parameters[index + 2] ?? "");
      if (tone) {
        return tone;
      }
    }
  }
  return null;
}

function removeSgrColors(parameters: string[]): string[] {
  const remaining: string[] = [];

  for (let index = 0; index < parameters.length; index += 1) {
    const value = Number(parameters[index]);
    const colorMode = parameters[index + 1];
    if ((parameters[index] === "38" || parameters[index] === "48") && colorMode === "2") {
      index += 4;
      continue;
    }
    if ((parameters[index] === "38" || parameters[index] === "48") && colorMode === "5") {
      index += 2;
      continue;
    }
    if (
      (value >= 30 && value <= 39) ||
      (value >= 40 && value <= 49) ||
      (value >= 90 && value <= 97) ||
      (value >= 100 && value <= 107)
    ) {
      continue;
    }
    remaining.push(parameters[index] ?? "");
  }

  return remaining;
}

export function normalizeClaudeLightDiffOutput(data: string): string {
  return data.replace(/\u001b\[([0-9;]*)m/g, (sequence, parameterText: string) => {
    const parameters = parameterText === "" ? [] : parameterText.split(";");
    const tone = findClaudeDiffTone(parameters);
    if (!tone) {
      return sequence;
    }

    const colors = CLAUDE_LIGHT_DIFF_COLORS[tone];
    return `\u001b[${[
      ...removeSgrColors(parameters),
      ...colors.foreground,
      ...colors.background,
    ].join(";")}m`;
  });
}

function splitTrailingIncompleteCsi(data: string): [complete: string, pending: string] {
  const match = data.match(/\u001b(?:\[[0-9;?]*)?$/);
  if (!match) {
    return [data, ""];
  }

  const pending = match[0];
  return [data.slice(0, -pending.length), pending];
}

function createTheme(
  themeMode: "dark" | "light",
  themeOverride?: VibeTerminalTheme,
  transparentSurface = false,
) {
  const baseTheme =
    themeMode === "light"
      ? {
          background: "#f8fafc",
          black: "#334155",
          blue: "#2563eb",
          brightBlack: "#64748b",
          brightBlue: "#3b82f6",
          brightCyan: "#06b6d4",
          brightGreen: "#16a34a",
          brightMagenta: "#c026d3",
          brightRed: "#dc2626",
          brightWhite: "#0f172a",
          brightYellow: "#ca8a04",
          cyan: "#0891b2",
          foreground: "#0f172a",
          green: "#15803d",
          magenta: "#a21caf",
          red: "#b91c1c",
          white: "#475569",
          yellow: "#a16207",
        }
      : {
          background: "#002b36",
          black: "#073642",
          blue: "#268bd2",
          brightBlack: "#586e75",
          brightBlue: "#839496",
          brightCyan: "#2aa198",
          brightGreen: "#859900",
          brightMagenta: "#d33682",
          brightRed: "#dc322f",
          brightWhite: "#fdf6e3",
          brightYellow: "#b58900",
          cyan: "#2aa198",
          foreground: "#d8e2dc",
          green: "#859900",
          magenta: "#6c71c4",
          red: "#dc322f",
          white: "#93a1a1",
          yellow: "#b58900",
        };

  // xterm draws its own scrollbar slider, so the slider colours have to travel
  // through the theme instead of CSS. The overview ruler border would otherwise
  // paint a 1px line next to the slider once `overviewRuler.width` is set.
  const scrollbarTheme = transparentSurface
    ? {
        overviewRulerBorder: "rgba(0, 0, 0, 0)",
        scrollbarSliderActiveBackground: "rgba(226, 232, 240, 0.7)",
        scrollbarSliderBackground: "rgba(203, 213, 225, 0.4)",
        scrollbarSliderHoverBackground: "rgba(226, 232, 240, 0.55)",
      }
    : themeMode === "light"
      ? {
          overviewRulerBorder: "rgba(0, 0, 0, 0)",
          scrollbarSliderActiveBackground: "rgba(68, 64, 60, 0.6)",
          scrollbarSliderBackground: "rgba(68, 64, 60, 0.32)",
          scrollbarSliderHoverBackground: "rgba(68, 64, 60, 0.48)",
        }
      : {
          overviewRulerBorder: "rgba(0, 0, 0, 0)",
          scrollbarSliderActiveBackground: "rgba(147, 161, 161, 0.68)",
          scrollbarSliderBackground: "rgba(147, 161, 161, 0.42)",
          scrollbarSliderHoverBackground: "rgba(147, 161, 161, 0.55)",
        };

  return {
    ...baseTheme,
    ...themeOverride,
    ...scrollbarTheme,
    ...(transparentSurface ? { background: "transparent" } : {}),
  };
}

export function XtermPane({
  session,
  active = true,
  themeMode = "dark",
  themeOverride,
  transparentSurface = false,
  onStatusChange,
}: XtermPaneProps) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const terminalRef = useRef<Terminal | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const theme = useMemo(
    () => createTheme(themeMode, themeOverride, transparentSurface),
    [themeMode, themeOverride, transparentSurface],
  );
  const normalizeClaudeDiffRef = useRef(false);
  const pendingClaudeOutputRef = useRef("");
  normalizeClaudeDiffRef.current =
    session.platform?.trim().toLowerCase() === "claude" &&
    themeMode === "light" &&
    !transparentSurface;
  const scrollbarClass = transparentSurface
    ? "xterm-pane-scrollbar-skin"
    : themeMode === "dark"
      ? "xterm-pane-scrollbar-dark"
      : "xterm-pane-scrollbar-light";

  useEffect(() => {
    const host = hostRef.current;
    if (!host) {
      return;
    }

    const terminal = new Terminal({
      allowProposedApi: false,
      allowTransparency: transparentSurface,
      convertEol: true,
      cursorBlink: true,
      fontFamily: '"JetBrains Mono", "Cascadia Code", "SFMono-Regular", Consolas, monospace',
      fontSize: 13,
      // xterm sizes its custom scrollbar slider from this width (default 14px),
      // so CSS cannot make the slider thinner.
      overviewRuler: { showBottomBorder: false, showTopBorder: false, width: 8 },
      // Agent sessions easily exceed xterm's 1000-line default. Claude also
      // redraws with ED2, so preserve the erased viewport in scrollback.
      scrollback: 10_000,
      scrollOnEraseInDisplay: true,
      theme,
    });
    const fitAddon = new FitAddon();
    terminal.loadAddon(fitAddon);
    terminal.open(host);
    terminalRef.current = terminal;
    fitAddonRef.current = fitAddon;

    const fitAndResize = () => {
      try {
        fitAddon.fit();
        void resizeTerminal(session.id, terminal.cols, terminal.rows).catch((error) => {
          terminal.writeln(`\r\n[resize failed] ${String(error)}`);
        });
      } catch {
        // Hidden panes can have zero dimensions; they are fitted again when activated.
      }
    };

    const inputDisposable = terminal.onData((data) => {
      void writeTerminalInput(session.id, data).catch((error) => {
        terminal.writeln(`\r\n[input failed] ${String(error)}`);
        onStatusChange?.(session.id, "error");
      });
    });

    let disposed = false;
    const transport = getTransport();
    const outputUnlisten = transport.subscribe<TerminalOutputEvent>("terminal://output", (payload) => {
      if (payload.sessionId === session.id) {
        if (!normalizeClaudeDiffRef.current) {
          const pending = pendingClaudeOutputRef.current;
          pendingClaudeOutputRef.current = "";
          terminal.write(`${pending}${payload.data}`);
          return;
        }

        const [complete, pending] = splitTrailingIncompleteCsi(
          `${pendingClaudeOutputRef.current}${payload.data}`,
        );
        pendingClaudeOutputRef.current = pending;
        if (complete) {
          terminal.write(normalizeClaudeLightDiffOutput(complete));
        }
      }
    });
    const exitUnlisten = transport.subscribe<TerminalExitEvent>("terminal://exit", (payload) => {
      if (payload.sessionId === session.id) {
        terminal.writeln(`\r\n[process exited: ${payload.exitCode ?? "unknown"}]`);
        onStatusChange?.(session.id, "exited", payload.exitCode ?? null);
      }
    });
    const errorUnlisten = transport.subscribe<TerminalErrorEvent>("terminal://error", (payload) => {
      if (payload.sessionId === session.id) {
        terminal.writeln(`\r\n[terminal error] ${payload.message}`);
        onStatusChange?.(session.id, "error");
      }
    });

    for (const unlisten of [outputUnlisten, exitUnlisten, errorUnlisten]) {
      void unlisten
        .then((cleanup) => {
          if (disposed) {
            cleanup();
          }
        })
        .catch((error) => {
          terminal.writeln(`\r\n[event listener failed] ${String(error)}`);
        });
    }

    const resizeObserver =
      typeof ResizeObserver === "undefined" ? null : new ResizeObserver(() => fitAndResize());
    resizeObserver?.observe(host);
    window.addEventListener("resize", fitAndResize);
    const frame = window.requestAnimationFrame(fitAndResize);

    return () => {
      disposed = true;
      window.cancelAnimationFrame(frame);
      window.removeEventListener("resize", fitAndResize);
      resizeObserver?.disconnect();
      inputDisposable.dispose();
      for (const unlisten of [outputUnlisten, exitUnlisten, errorUnlisten]) {
        void unlisten.then((cleanup) => cleanup()).catch(() => undefined);
      }
      pendingClaudeOutputRef.current = "";
      terminal.dispose();
      terminalRef.current = null;
      fitAddonRef.current = null;
    };
  }, [onStatusChange, session.id]);

  useEffect(() => {
    const terminal = terminalRef.current;
    if (!terminal) {
      return;
    }
    terminal.options.theme = theme;
    terminal.refresh(0, terminal.rows - 1);
  }, [theme]);

  useEffect(() => {
    if (!active) {
      return;
    }
    window.requestAnimationFrame(() => {
      try {
        fitAddonRef.current?.fit();
      } catch {
        // The pane may still be calculating layout.
      }
      terminalRef.current?.focus();
    });
  }, [active]);

  return (
    <div
      aria-label={`${session.title} terminal`}
      className={`xterm-pane ${scrollbarClass} h-full min-h-0 ${transparentSurface ? "xterm-pane-skin-transparent" : ""} ${
        active ? "block" : "hidden"
      }`}
      ref={hostRef}
    />
  );
}
