import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef } from "react";
import { resizeTerminal, writeTerminalInput } from "../../lib/api/client";
import type {
  TerminalErrorEvent,
  TerminalExitEvent,
  TerminalOutputEvent,
  TerminalSession,
  TerminalStatus,
} from "../../lib/api/types";

type XtermPaneProps = {
  session: TerminalSession;
  active?: boolean;
  onStatusChange?: (sessionId: string, status: TerminalStatus) => void;
};

export function XtermPane({
  session,
  active = true,
  onStatusChange,
}: XtermPaneProps) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const terminalRef = useRef<Terminal | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);

  useEffect(() => {
    const host = hostRef.current;
    if (!host) {
      return;
    }

    const terminal = new Terminal({
      allowProposedApi: false,
      cursorBlink: true,
      convertEol: true,
      fontFamily: '"JetBrains Mono", "Cascadia Code", "SFMono-Regular", Consolas, monospace',
      fontSize: 13,
      theme: {
        background: "#10100f",
        black: "#27272a",
        blue: "#38bdf8",
        brightBlack: "#52525b",
        brightBlue: "#7dd3fc",
        brightCyan: "#67e8f9",
        brightGreen: "#86efac",
        brightMagenta: "#f0abfc",
        brightRed: "#fca5a5",
        brightWhite: "#fafafa",
        brightYellow: "#fde68a",
        cyan: "#22d3ee",
        foreground: "#f4f4f5",
        green: "#4ade80",
        magenta: "#e879f9",
        red: "#f87171",
        white: "#e4e4e7",
        yellow: "#fbbf24",
      },
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
    const outputUnlisten = listen<TerminalOutputEvent>("terminal://output", (event) => {
      if (event.payload.sessionId === session.id) {
        terminal.write(event.payload.data);
      }
    });
    const exitUnlisten = listen<TerminalExitEvent>("terminal://exit", (event) => {
      if (event.payload.sessionId === session.id) {
        terminal.writeln(`\r\n[process exited: ${event.payload.exitCode ?? "unknown"}]`);
        onStatusChange?.(session.id, "exited");
      }
    });
    const errorUnlisten = listen<TerminalErrorEvent>("terminal://error", (event) => {
      if (event.payload.sessionId === session.id) {
        terminal.writeln(`\r\n[terminal error] ${event.payload.message}`);
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
      typeof ResizeObserver === "undefined"
        ? null
        : new ResizeObserver(() => fitAndResize());
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
      className={`h-full min-h-0 ${active ? "block" : "hidden"}`}
      ref={hostRef}
    />
  );
}
