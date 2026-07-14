import type { Transport, Unsubscribe } from "./types";

type WebEvent = {
  channel: string;
  payload: unknown;
};

export const WEB_TOKEN_STORAGE_KEY = "ai-switch.webToken";

export function getWebAccessToken() {
  if (typeof window === "undefined") {
    return "";
  }

  return window.localStorage.getItem(WEB_TOKEN_STORAGE_KEY) ?? "";
}

export function setWebAccessToken(token: string) {
  if (typeof window === "undefined") {
    return;
  }

  const value = token.trim();
  if (!value) {
    window.localStorage.removeItem(WEB_TOKEN_STORAGE_KEY);
    return;
  }

  window.localStorage.setItem(WEB_TOKEN_STORAGE_KEY, value);
}

export function clearWebAccessToken() {
  if (typeof window === "undefined") {
    return;
  }

  window.localStorage.removeItem(WEB_TOKEN_STORAGE_KEY);
}

export function isUnauthorizedTransportError(error: unknown) {
  if (!(error instanceof Error)) {
    return false;
  }

  const message = error.message.trim().toLowerCase();
  return message === "unauthorized" || message.includes("http 401");
}

function websocketUrl(baseUrl: string) {
  return `${baseUrl.replace(/^http/, "ws").replace(/\/$/, "")}/ws/events`;
}

export class WebTransport implements Transport {
  private readonly baseUrl: string;
  private readonly handlers = new Map<string, Set<(payload: unknown) => void>>();
  private socket: WebSocket | null = null;

  constructor(baseUrl = typeof window === "undefined" ? "" : window.location.origin) {
    this.baseUrl = baseUrl.replace(/\/$/, "");
  }

  async call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
    const token = getWebAccessToken();
    const response = await fetch(`${this.baseUrl}/api/${command}`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        ...(token ? { Authorization: `Bearer ${token}` } : {}),
      },
      body: JSON.stringify(args ?? {}),
    });

    if (!response.ok) {
      const payload = await response.json().catch(() => null);
      const message =
        payload && typeof payload === "object" && "message" in payload
          ? String(payload.message)
          : `HTTP ${response.status}`;
      throw new Error(message);
    }

    return response.json() as Promise<T>;
  }

  async subscribe<T>(event: string, handler: (payload: T) => void): Promise<Unsubscribe> {
    const wrapped = handler as (payload: unknown) => void;
    const handlers = this.handlers.get(event) ?? new Set<(payload: unknown) => void>();
    handlers.add(wrapped);
    this.handlers.set(event, handlers);
    this.ensureSocket();

    return () => {
      this.handlers.get(event)?.delete(wrapped);
    };
  }

  isDesktop() {
    return false;
  }

  destroy() {
    this.socket?.close();
    this.socket = null;
    this.handlers.clear();
  }

  private ensureSocket() {
    if (this.socket && this.socket.readyState <= WebSocket.OPEN) {
      return;
    }

    const token = getWebAccessToken();
    const url = new URL(websocketUrl(this.baseUrl));
    if (token) {
      url.searchParams.set("token", token);
    }

    this.socket = new WebSocket(url.toString());
    this.socket.onmessage = (message) => {
      const event = JSON.parse(message.data) as WebEvent;
      const handlers = this.handlers.get(event.channel);
      if (!handlers) {
        return;
      }

      for (const handler of handlers) {
        handler(event.payload);
      }
    };
    this.socket.onclose = () => {
      this.socket = null;
    };
  }
}
