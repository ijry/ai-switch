import type { Transport, Unsubscribe } from "./types";

export class TauriTransport implements Transport {
  async call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<T>(command, args);
  }

  async subscribe<T>(event: string, handler: (payload: T) => void): Promise<Unsubscribe> {
    const { listen } = await import("@tauri-apps/api/event");
    return listen<T>(event, (message) => {
      handler(message.payload);
    });
  }

  isDesktop() {
    return true;
  }
}
