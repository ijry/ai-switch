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
  onStatusChange?: (sessionId: string, status: TerminalStatus) => void;
};

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

  return {
    ...baseTheme,
    ...themeOverride,
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
        terminal.write(payload.data);
      }
    });
    const exitUnlisten = transport.subscribe<TerminalExitEvent>("terminal://exit", (payload) => {
      if (payload.sessionId === session.id) {
        terminal.writeln(`\r\n[process exited: ${payload.exitCode ?? "unknown"}]`);
        onStatusChange?.(session.id, "exited");
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
      className={`xterm-pane h-full min-h-0 ${transparentSurface ? "xterm-pane-skin-transparent" : ""} ${
        active ? "block" : "hidden"
      }`}
      ref={hostRef}
    />
  );
}
