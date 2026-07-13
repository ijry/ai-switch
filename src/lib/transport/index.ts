import { isTauriRuntime } from "./detect";
import { TauriTransport } from "./tauri-transport";
import { WebTransport } from "./web-transport";
import type { Transport, Unsubscribe } from "./types";

export type { Transport, Unsubscribe };

let transport: Transport | null = null;

function createTransport() {
  if (isTauriRuntime()) {
    return new TauriTransport();
  }

  return new WebTransport();
}

export function getTransport() {
  transport ??= createTransport();
  return transport;
}

export function isDesktop() {
  return getTransport().isDesktop();
}

export function __resetTransportForTests() {
  if (process.env.NODE_ENV !== "test") {
    return;
  }

  transport?.destroy?.();
  transport = null;
}

export { TauriTransport } from "./tauri-transport";
export { WebTransport } from "./web-transport";
