import type { Transport, Unsubscribe } from "./types";

export type { Transport, Unsubscribe };

export {
  WEB_TOKEN_STORAGE_KEY,
  clearWebAccessToken,
  getWebAccessToken,
  isUnauthorizedTransportError,
  setWebAccessToken,
  WebTransport,
} from "./web-transport";
export { TauriTransport } from "./tauri-transport";
export { isLocalWebDevRuntime, isTauriRuntime } from "./detect";

import { isTauriRuntime } from "./detect";
import { TauriTransport } from "./tauri-transport";
import { WebTransport } from "./web-transport";

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
