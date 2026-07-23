type TauriWindow = Window & {
  __TAURI_INTERNALS__?: unknown;
  isTauri?: boolean;
};

export function isTauriRuntime() {
  if (typeof window === "undefined") {
    return false;
  }

  const tauriWindow = window as TauriWindow;
  const tauriGlobal = globalThis as typeof globalThis & { isTauri?: boolean };

  return Boolean(tauriWindow.__TAURI_INTERNALS__ || tauriWindow.isTauri || tauriGlobal.isTauri);
}

export function isLocalWebDevRuntime() {
  const viteEnv = (import.meta as ImportMeta & { env?: { DEV?: boolean } }).env;

  if (typeof window === "undefined" || !viteEnv?.DEV) {
    return false;
  }

  return ["localhost", "127.0.0.1", "::1"].includes(window.location.hostname);
}
