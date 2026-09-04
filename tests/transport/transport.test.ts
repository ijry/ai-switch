import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  __resetTransportForTests,
  getTransport,
  isDesktop,
  isLocalWebDevRuntime,
  setWebAccessToken,
  TauriTransport,
  WebTransport,
} from "../../src/lib/transport";

type TauriWindow = Window & {
  __TAURI_INTERNALS__?: {
    invoke?: ReturnType<typeof vi.fn>;
    transformCallback?: ReturnType<typeof vi.fn>;
  };
  __TAURI_EVENT_PLUGIN_INTERNALS__?: {
    unregisterListener?: ReturnType<typeof vi.fn>;
  };
  isTauri?: boolean;
};

type TestTauriEvent = { event: string; id: number; payload: string };

class FakeWebSocket {
  static readonly CONNECTING = 0;
  static readonly OPEN = 1;
  static readonly CLOSING = 2;
  static readonly CLOSED = 3;

  static instances: FakeWebSocket[] = [];

  readyState = FakeWebSocket.OPEN;
  onopen: (() => void) | null = null;
  onmessage: ((message: { data: string }) => void) | null = null;
  onerror: (() => void) | null = null;
  onclose: (() => void) | null = null;

  constructor(readonly url: string) {
    FakeWebSocket.instances.push(this);
  }

  close() {
    this.readyState = FakeWebSocket.CLOSED;
  }
}

function stubWebSocket() {
  FakeWebSocket.instances = [];
  vi.stubGlobal("WebSocket", FakeWebSocket);
  return FakeWebSocket.instances;
}

describe("transport", () => {
  beforeEach(() => {
    __resetTransportForTests();
    delete (window as TauriWindow).__TAURI_INTERNALS__;
    delete (window as TauriWindow).__TAURI_EVENT_PLUGIN_INTERNALS__;
    delete (window as TauriWindow).isTauri;
    window.localStorage.clear();
    vi.unstubAllGlobals();
  });

  it("uses web transport outside Tauri", () => {
    expect(isDesktop()).toBe(false);
    expect(getTransport().isDesktop()).toBe(false);
  });

  it("detects localhost Vite as local web dev runtime", () => {
    expect(window.location.hostname).toBe("localhost");
    expect(isLocalWebDevRuntime()).toBe(true);
    expect(isDesktop()).toBe(false);
  });

  it("uses tauri transport when the Tauri v2 runtime flag is present", () => {
    (window as TauriWindow).isTauri = true;

    expect(isDesktop()).toBe(true);
    expect(getTransport().isDesktop()).toBe(true);
  });

  it("calls Tauri commands through injected IPC without dynamic imports", async () => {
    const response = [{ id: "claude-account" }];
    const invoke = vi.fn().mockResolvedValue(response);
    (window as TauriWindow).__TAURI_INTERNALS__ = { invoke };

    await expect(getTransport().call("list_route_credentials", { platform: "claude" })).resolves.toEqual(response);

    expect(invoke).toHaveBeenCalledWith("list_route_credentials", { platform: "claude" });
  });

  it("preserves Tauri API error fields", async () => {
    const invoke = vi.fn().mockRejectedValue({
      code: "capability.unavailable",
      message: "Not supported",
      details: "hermes:config_write",
      recoverable: true,
      operation_id: "operation-1",
    });
    (window as TauriWindow).__TAURI_INTERNALS__ = { invoke };

    await expect(new TauriTransport().call("write_route_proxy_configs")).rejects.toMatchObject({
      name: "ApiClientError",
      code: "capability.unavailable",
      details: "hermes:config_write",
      recoverable: true,
      operationId: "operation-1",
    });
  });

  it("subscribes to Tauri events through injected IPC", async () => {
    let tauriCallback: (message: TestTauriEvent) => void = () => {};
    const invoke = vi.fn((command: string) =>
      command === "plugin:event|listen" ? Promise.resolve(42) : Promise.resolve(undefined),
    );
    const transformCallback = vi.fn((callback: (message: TestTauriEvent) => void) => {
      tauriCallback = callback;
      return 7;
    });
    const unregisterListener = vi.fn();
    (window as TauriWindow).__TAURI_INTERNALS__ = { invoke, transformCallback };
    (window as TauriWindow).__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener };
    const handler = vi.fn();

    const unsubscribe = await new TauriTransport().subscribe<string>("terminal://output", handler);
    tauriCallback({ event: "terminal://output", id: 42, payload: "ok" });
    unsubscribe();

    expect(transformCallback).toHaveBeenCalledWith(expect.any(Function));
    expect(invoke).toHaveBeenCalledWith("plugin:event|listen", {
      event: "terminal://output",
      target: { kind: "Any" },
      handler: 7,
    });
    expect(handler).toHaveBeenCalledWith("ok");
    expect(unregisterListener).toHaveBeenCalledWith("terminal://output", 42);
    expect(invoke).toHaveBeenCalledWith("plugin:event|unlisten", {
      event: "terminal://output",
      eventId: 42,
    });
  });

  it("posts command calls to the web api with the saved token", async () => {
    setWebAccessToken("secret-token");
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ ok: true }), {
        headers: { "Content-Type": "application/json" },
        status: 200,
      }),
    );
    vi.stubGlobal("fetch", fetchMock);

    const transport = new WebTransport("http://127.0.0.1:3090");
    await expect(transport.call("get_settings", { scope: "test" })).resolves.toEqual({ ok: true });

    expect(fetchMock).toHaveBeenCalledWith("http://127.0.0.1:3090/api/get_settings", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Authorization: "Bearer secret-token",
      },
      body: JSON.stringify({ scope: "test" }),
    });
  });

  it("preserves Web API error fields", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(
        new Response(
          JSON.stringify({
            code: "capability.unavailable",
            message: "Not supported",
            details: "hermes:config_write",
            recoverable: true,
            operation_id: "operation-2",
          }),
          { status: 400, headers: { "Content-Type": "application/json" } },
        ),
      ),
    );

    await expect(
      new WebTransport("http://127.0.0.1:3090").call("write_route_proxy_configs"),
    ).rejects.toEqual(
      expect.objectContaining({
        code: "capability.unavailable",
        details: "hermes:config_write",
        recoverable: true,
        operationId: "operation-2",
      }),
    );
  });

  it("never dispatches desktop-only save commands over Web transport", async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal("fetch", fetchMock);

    await expect(
      new WebTransport("http://127.0.0.1:3090").call("save_route_credential_export", {
        suggested_file_name: "route-credentials.json",
        json_text: "[]",
      }),
    ).rejects.toMatchObject({
      name: "ApiClientError",
      code: "transport.desktop_only",
      recoverable: false,
    });
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("normalizes Web network failures", async () => {
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue("connection refused"));

    await expect(new WebTransport("http://127.0.0.1:3090").call("get_settings")).rejects.toMatchObject({
      name: "ApiClientError",
      code: "transport.error",
      message: "connection refused",
      recoverable: true,
    });
  });

  it("posts start_tailscale_with_auth_key with bearer token", async () => {
    setWebAccessToken("secret-token");
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(
        JSON.stringify({
          state: "connected",
          accessUrls: ["http://100.64.0.12:3090"],
        }),
        {
          headers: { "Content-Type": "application/json" },
          status: 200,
        },
      ),
    );
    vi.stubGlobal("fetch", fetchMock);

    const transport = new WebTransport("http://127.0.0.1:3090");
    await expect(
      transport.call("start_tailscale_with_auth_key", { authKey: "tskey-auth-test" }),
    ).resolves.toEqual({
      state: "connected",
      accessUrls: ["http://100.64.0.12:3090"],
    });

    expect(fetchMock).toHaveBeenCalledWith(
      "http://127.0.0.1:3090/api/start_tailscale_with_auth_key",
      {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          Authorization: "Bearer secret-token",
        },
        body: JSON.stringify({ authKey: "tskey-auth-test" }),
      },
    );
  });

  it("reconnects the event socket after an unexpected close", async () => {
    vi.useFakeTimers();
    const sockets = stubWebSocket();
    const transport = new WebTransport("http://127.0.0.1:3090");

    await transport.subscribe("route-credential-status", () => {});
    expect(sockets).toHaveLength(1);

    // One dropped connection used to end live updates for good; the UI just
    // looked like the data had stopped moving.
    sockets[0].onclose?.();
    await vi.advanceTimersByTimeAsync(1000);
    expect(sockets).toHaveLength(2);

    transport.destroy();
    await vi.advanceTimersByTimeAsync(60000);
    expect(sockets).toHaveLength(2);
    vi.useRealTimers();
  });

  it("stops reconnecting once the last subscriber has unsubscribed", async () => {
    // `scheduleReconnect` bails when no channel has handlers. That guard was
    // unreachable while unsubscribe only emptied the Set and left the key behind,
    // so a screen the user had navigated away from kept the singleton rebuilding
    // a socket every 15s forever — no attempt cap, nothing shown.
    vi.useFakeTimers();
    const sockets = stubWebSocket();
    const transport = new WebTransport("http://127.0.0.1:3090");

    const unsubscribe = await transport.subscribe("route-credential-status", () => {});
    expect(sockets).toHaveLength(1);

    unsubscribe();
    sockets[0].onclose?.();
    await vi.advanceTimersByTimeAsync(60000);

    expect(sockets).toHaveLength(1);
    transport.destroy();
    vi.useRealTimers();
  });

  it("ignores a frame that is not JSON instead of throwing", async () => {
    const sockets = stubWebSocket();
    const transport = new WebTransport("http://127.0.0.1:3090");
    const handler = vi.fn();
    await transport.subscribe("route-credential-status", handler);

    expect(() => sockets[0].onmessage?.({ data: "not json" })).not.toThrow();
    expect(handler).not.toHaveBeenCalled();

    sockets[0].onmessage?.({
      data: JSON.stringify({ channel: "route-credential-status", payload: { id: "a" } }),
    });
    expect(handler).toHaveBeenCalledWith({ id: "a" });
    transport.destroy();
  });
});
