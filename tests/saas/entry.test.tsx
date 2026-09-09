import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { SaasPortal } from "../../src/saas/entry";
import { group, key, overview, page, publicConfig, recharge, user } from "./fixtures";

const fetcher = vi.fn();
let authenticated = false;
let failure = "";
beforeEach(() => {
  localStorage.clear();
  localStorage.setItem("saas.locale", "en");
  window.history.replaceState({}, "", "/");
  authenticated = false;
  failure = "";
  fetcher.mockReset().mockImplementation(async (path: string) => {
    if (path.endsWith("public/config")) return Response.json(publicConfig);
    if (path.endsWith("auth/session")) return Response.json({ user: authenticated ? user : null, csrfToken: authenticated ? "csrf" : null });
    if (failure) return Response.json({ code: "rejected", message: failure }, { status: 400 });
    if (path.endsWith("keys.create") || path.endsWith("keys.rotate")) return Response.json({ ...key, plaintextKey: "sk-saas-secret-once" });
    if (path.endsWith("keys.list")) return Response.json(page([key]));
    if (path.endsWith("groups")) return Response.json(page([group]));
    if (path.endsWith("recharges.list")) return Response.json(page([recharge]));
    if (path.endsWith("redeem")) return Response.json({ amountMicros: 1000000, balanceMicros: 13500000 });
    if (path.endsWith("logs.query")) return Response.json(page());
    if (path.endsWith("overview")) return Response.json(overview);
    return Response.json({});
  });
  vi.stubGlobal("fetch", fetcher);
});
afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

describe("SaaS user portal", () => {
  it("shows GitHub login without fetching private data", async () => {
    render(<SaasPortal />);
    expect(await screen.findByRole("button", { name: /^sign in$/i })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /continue with github/i })).toHaveAttribute("href", "/api/saas/auth/github");
    expect(fetcher.mock.calls.some(([path]) => String(path).includes("/user/"))).toBe(false);
    expect(screen.queryByText("sk-saas-secret-once")).not.toBeInTheDocument();
  });

  it("logs in with an administrator-created email account", async () => {
    fetcher.mockImplementation(async (path: string) => {
      if (path.endsWith("public/config")) return Response.json(publicConfig);
      if (path.endsWith("auth/session")) return Response.json({ user: null, csrfToken: null });
      if (path.endsWith("auth/password")) return Response.json({ user, csrfToken: "csrf" });
      if (path.endsWith("overview")) return Response.json(overview);
      return Response.json(page());
    });
    const actor = userEvent.setup();
    render(<SaasPortal />);
    await actor.type(await screen.findByLabelText(/^email$/i), "member@example.com");
    await actor.type(screen.getByLabelText(/^password$/i), "secret-pass");
    await actor.click(screen.getByRole("button", { name: /^sign in$/i }));
    await waitFor(() => expect(fetcher).toHaveBeenCalledWith("/api/saas/auth/password", expect.objectContaining({
      body: JSON.stringify({ email: "member@example.com", password: "secret-pass" }),
      headers: { "Content-Type": "application/json" },
    })));
  });

  it("renders the actual wallet and aggregates, not seeded statistics", async () => {
    authenticated = true;
    render(<SaasPortal />);
    expect(await screen.findByText("$12.50")).toBeInTheDocument();
    expect(await screen.findByText("$1.234567")).toBeInTheDocument();
    expect(screen.getByRole("navigation", { name: /workspace/i })).toBeInTheDocument();
  });

  it("creates a key with a fixed group and forgets the secret after closing", async () => {
    authenticated = true;
    window.history.replaceState({}, "", "/api-keys");
    const actor = userEvent.setup();
    render(<SaasPortal />);
    await actor.click(await screen.findByRole("button", { name: /^create api key$/i }));
    await actor.type(screen.getByLabelText(/^key name$/i), "Laptop");
    await actor.selectOptions(screen.getByLabelText(/^group$/i), "g1");
    await actor.click(screen.getByRole("button", { name: /^create key$/i }));
    expect(await screen.findByText("sk-saas-secret-once")).toBeInTheDocument();
    expect(fetcher).toHaveBeenCalledWith("/api/saas/user/keys.create", expect.objectContaining({ body: expect.stringContaining('"groupId":"g1"') }));
    await actor.click(screen.getByRole("button", { name: /saved.*close/i }));
    expect(screen.queryByText("sk-saas-secret-once")).not.toBeInTheDocument();
  });

  it("cancels a pending manual recharge only after confirmation", async () => {
    authenticated = true;
    window.history.replaceState({}, "", "/recharge");
    const actor = userEvent.setup();
    render(<SaasPortal />);
    await actor.click(await screen.findByRole("button", { name: /cancel request/i }));
    expect(fetcher.mock.calls.some(([path]) => path.endsWith("recharges.cancel"))).toBe(false);
    await actor.click(screen.getByRole("button", { name: /confirm cancellation/i }));
    await waitFor(() => expect(fetcher).toHaveBeenCalledWith("/api/saas/user/recharges.cancel", expect.objectContaining({ body: JSON.stringify({ id: "r1" }) })));
  });

  it("shows a server error instead of a successful redemption", async () => {
    authenticated = true;
    window.history.replaceState({}, "", "/redeem");
    const actor = userEvent.setup();
    render(<SaasPortal />);
    await actor.type(await screen.findByLabelText(/redemption code/i), "used-code");
    failure = "This code has already been used.";
    await actor.click(screen.getByRole("button", { name: /^redeem now$/i }));
    expect(await screen.findByRole("alert")).toHaveTextContent(failure);
    expect(screen.queryByText(/credited to your balance/i)).not.toBeInTheDocument();
  });

  it("refreshes confirmed balance after a successful redemption", async () => {
    authenticated = true;
    window.history.replaceState({}, "", "/redeem");
    const actor = userEvent.setup();
    render(<SaasPortal />);
    await actor.type(await screen.findByLabelText(/redemption code/i), "valid-code");
    await actor.click(screen.getByRole("button", { name: /^redeem now$/i }));
    expect(await screen.findByText(/redeemed.*credited/i)).toHaveTextContent("$1.00");
    await waitFor(() => expect(fetcher.mock.calls.filter(([path]) => path.endsWith("overview")).length).toBeGreaterThan(1));
  });
});
