import { beforeEach, describe, expect, it, vi } from "vitest";
import { adminCall, createUserClient, SaasApiError } from "../../src/saas/api";
import { decimalToInteger, estimateRecharge, formatMoney } from "../../src/saas/format";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("../../src/lib/transport", () => ({ getTransport: () => ({ call: invoke }) }));

describe("SaaS isolated transport", () => {
  beforeEach(() => vi.clearAllMocks());

  it("never requests private resources without a session", async () => {
    const fetcher = vi.fn();
    const client = createUserClient(fetcher);
    await expect(client.call("keys.list")).rejects.toMatchObject({ code: "unauthenticated" });
    expect(fetcher).not.toHaveBeenCalled();
    expect(invoke).not.toHaveBeenCalled();
  });

  it("uses same-origin Cookies and the session CSRF, never administrator authorization", async () => {
    const fetcher = vi.fn()
      .mockResolvedValueOnce(new Response(JSON.stringify({ user: { id: "u1" }, csrfToken: "csrf-only" })))
      .mockResolvedValueOnce(new Response(JSON.stringify({ items: [], total: 0 })));
    const client = createUserClient(fetcher);
    await client.session();
    await client.call("keys.list", { page: 1, pageSize: 20 });
    expect(fetcher).toHaveBeenLastCalledWith("/api/saas/user/keys.list", expect.objectContaining({
      method: "POST", credentials: "same-origin", cache: "no-store",
      headers: { "Content-Type": "application/json", "x-saas-csrf": "csrf-only" },
      body: JSON.stringify({ page: 1, pageSize: 20 }),
    }));
    expect(invoke).not.toHaveBeenCalled();
  });

  it("expires the private session on 401 and does not claim success", async () => {
    const expired = vi.fn();
    const fetcher = vi.fn()
      .mockResolvedValueOnce(new Response(JSON.stringify({ user: { id: "u1" }, csrfToken: "csrf" })))
      .mockResolvedValueOnce(new Response(JSON.stringify({ code: "session_expired", message: "Sign in again" }), { status: 401 }));
    const client = createUserClient(fetcher, expired);
    await client.session();
    await expect(client.call("redeem", { code: "test" })).rejects.toBeInstanceOf(SaasApiError);
    expect(expired).toHaveBeenCalledOnce();
    await expect(client.call("overview")).rejects.toMatchObject({ code: "unauthenticated" });
    expect(fetcher).toHaveBeenCalledTimes(2);
  });

  it("routes administrator operations exclusively through the trusted adapter", async () => {
    invoke.mockResolvedValue({ items: [], total: 0 });
    await adminCall("users.list", { page: 1 });
    expect(invoke).toHaveBeenCalledWith("saas_admin", { operation: "users.list", payload: { page: 1 } });
  });

  it("retains structured server failure codes", async () => {
    const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify({ code: "saas_disabled", message: "Disabled" }), { status: 503 }));
    await expect(createUserClient(fetcher).publicConfig()).rejects.toMatchObject({ code: "saas_disabled", message: "Disabled" });
  });
});

describe("fixed point presentation", () => {
  it("parses decimal amounts exactly and rejects rounding, negatives and overflow", () => {
    expect(decimalToInteger("0.000001", 6)).toBe(1);
    expect(decimalToInteger("12.34", 2)).toBe(1234);
    expect(() => decimalToInteger("1.001", 2)).toThrow();
    expect(() => decimalToInteger("-2", 6)).toThrow();
    expect(() => decimalToInteger("NaN", 6)).toThrow();
    expect(() => decimalToInteger("9007199254740992", 6)).toThrow();
  });

  it("floors an informational recharge estimate with integer arithmetic", () => {
    expect(estimateRecharge(100, 7000000)).toBe(142857);
    expect(formatMoney(1234567, "en")).toBe("$1.234567");
    expect(formatMoney(undefined, "en")).toBe("—");
  });
});
