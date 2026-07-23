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
});
