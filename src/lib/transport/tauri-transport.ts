import type { Transport, Unsubscribe } from "./types";

type TauriInvoke = <T>(
  command: string,
  args?: Record<string, unknown>,
  options?: unknown,
) => Promise<T>;

type TauriInternals = {
  invoke?: TauriInvoke;
  transformCallback?: <T>(callback: (response: T) => void, once?: boolean) => number;
};

type ReadyTauriInternals = TauriInternals & {
  invoke: TauriInvoke;
};

type TauriEventPluginInternals = {
  unregisterListener?: (event: string, eventId: number) => void;
};

type TauriWindow = Window & {
  __TAURI_INTERNALS__?: TauriInternals;
  __TAURI_EVENT_PLUGIN_INTERNALS__?: TauriEventPluginInternals;
};

type TauriEvent<T> = {
  event: string;
  id: number;
  payload: T;
};

function getTauriWindow() {
  if (typeof window === "undefined") {
    throw new Error("Tauri IPC is unavailable outside a browser window.");
  }

  return window as TauriWindow;
}

function getTauriInternals(): ReadyTauriInternals {
  const internals = getTauriWindow().__TAURI_INTERNALS__;
  if (!internals?.invoke) {
    throw new Error("Tauri IPC is unavailable.");
  }

  return internals as ReadyTauriInternals;
}

export class TauriTransport implements Transport {
  async call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
    return getTauriInternals().invoke<T>(command, args ?? {});
  }

  async subscribe<T>(event: string, handler: (payload: T) => void): Promise<Unsubscribe> {
    const tauriWindow = getTauriWindow();
    const internals = getTauriInternals();
    const transformCallback = internals.transformCallback;
    if (!transformCallback) {
      throw new Error("Tauri event IPC is unavailable.");
    }

    const callbackId = transformCallback<TauriEvent<T>>((message) => {
      handler(message.payload);
    });
    const eventId = await internals.invoke<number>("plugin:event|listen", {
      event,
      target: { kind: "Any" },
      handler: callbackId,
    });

    return () => {
      tauriWindow.__TAURI_EVENT_PLUGIN_INTERNALS__?.unregisterListener?.(event, eventId);
      void internals.invoke("plugin:event|unlisten", { event, eventId }).catch(() => {});
    };
  }

  isDesktop() {
    return true;
  }
}
