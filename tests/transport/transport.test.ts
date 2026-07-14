import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  __resetTransportForTests,
  getTransport,
  isDesktop,
  setWebAccessToken,
  WebTransport,
} from "../../src/lib/transport";

type TauriWindow = Window & {
  __TAURI_INTERNALS__?: unknown;
};

describe("transport", () => {
  beforeEach(() => {
    __resetTransportForTests();
    delete (window as TauriWindow).__TAURI_INTERNALS__;
    window.localStorage.clear();
    vi.unstubAllGlobals();
  });

  it("uses web transport outside Tauri", () => {
    expect(isDesktop()).toBe(false);
    expect(getTransport().isDesktop()).toBe(false);
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
