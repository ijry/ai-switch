import type { Transport, Unsubscribe } from "./types";
import { ApiClientError, normalizeApiError } from "../api/errors";
import { isDesktopOnlyCommand } from "../api/commandSupport";

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
  if (error instanceof ApiClientError && error.code === "web.unauthorized") {
    return true;
  }
  if (!(error instanceof Error)) {
    return false;
  }

  const message = error.message.trim().toLowerCase();
  return message === "unauthorized" || message.includes("http 401");
}

export function websocketUrl(baseUrl: string, path = "/ws/events") {
  return `${baseUrl.replace(/^http/, "ws").replace(/\/$/, "")}${path}`;
}

export class WebTransport implements Transport {
  private readonly baseUrl: string;
  private readonly handlers = new Map<string, Set<(payload: unknown) => void>>();
  private socket: WebSocket | null = null;
  private reconnectAttempts = 0;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private destroyed = false;

  constructor(baseUrl = typeof window === "undefined" ? "" : window.location.origin) {
    this.baseUrl = baseUrl.replace(/\/$/, "");
  }

  async call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
    try {
      if (isDesktopOnlyCommand(command)) {
        throw new ApiClientError(
          "This command is only available in the desktop application.",
          "transport.desktop_only",
          command,
          false,
          null,
        );
      }
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
        const body = await response.text();
        let payload: unknown = body;
        if (body) {
          try {
            payload = JSON.parse(body) as unknown;
          } catch {
            payload = body;
          }
        }
        throw normalizeApiError(payload, `HTTP ${response.status}`, `web.http_${response.status}`);
      }

      return response.json() as Promise<T>;
    } catch (error) {
      throw normalizeApiError(error);
    }
  }

  async subscribe<T>(event: string, handler: (payload: T) => void): Promise<Unsubscribe> {
    const wrapped = handler as (payload: unknown) => void;
    const handlers = this.handlers.get(event) ?? new Set<(payload: unknown) => void>();
    handlers.add(wrapped);
    this.handlers.set(event, handlers);
    this.ensureSocket();

    return () => {
      const handlers = this.handlers.get(event);
      if (!handlers) {
        return;
      }
      handlers.delete(wrapped);
      // Drop the key, not just the callback. `scheduleReconnect` refuses to
      // reconnect once `handlers.size` reaches zero, and leaving empty Sets
      // behind made that guard unreachable: after every subscriber unmounted the
      // module-level singleton still rebuilt a socket every 15s forever, with no
      // attempt cap and nothing to show the user.
      if (handlers.size === 0) {
        this.handlers.delete(event);
      }
    };
  }

  isDesktop() {
    return false;
  }

  destroy() {
    this.destroyed = true;
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
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
    this.socket.onopen = () => {
      this.reconnectAttempts = 0;
    };
    this.socket.onmessage = (message) => {
      // One malformed frame must not take down the whole subscription chain.
      let event: WebEvent;
      try {
        event = JSON.parse(message.data as string) as WebEvent;
      } catch {
        return;
      }

      const handlers = this.handlers.get(event.channel);
      if (!handlers) {
        return;
      }

      for (const handler of handlers) {
        handler(event.payload);
      }
    };
    this.socket.onerror = () => {
      this.socket?.close();
    };
    this.socket.onclose = () => {
      this.socket = null;
      this.scheduleReconnect();
    };
  }

  /**
   * Without this, a single network blip ends live activity and status updates for
   * good — and the UI shows no error, it just stops moving.
   */
  private scheduleReconnect() {
    if (this.destroyed || this.handlers.size === 0 || this.reconnectTimer) {
      return;
    }

    const delay = Math.min(1000 * 2 ** Math.min(this.reconnectAttempts, 4), 15000);
    this.reconnectAttempts += 1;
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      this.ensureSocket();
    }, delay);
  }
}
