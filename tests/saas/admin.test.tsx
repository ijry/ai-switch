import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { SaasAdmin, SaasSettings } from "../../src/saas";
import { config, group, page, recharge, user } from "./fixtures";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("../../src/lib/transport", () => ({ getTransport: () => ({ call: invoke }), isDesktop: () => true }));
beforeEach(() => {
  localStorage.setItem("saas.locale", "en");
  invoke.mockReset().mockImplementation(async (_command: string, request?: { operation?: string }) => {
    const operation = request?.operation;
    if (_command === "get_web_server_status") return { running: true, host: "127.0.0.1", port: 10086, baseUrl: "http://127.0.0.1:10086" };
    if (operation === "config.get" || operation === "config.save") return config;
    if (operation === "activation.status") return { unlocked: false };
    if (operation === "activation.unlock") return { unlocked: true };
    if (operation === "groups.list") return page([group]);
    if (operation === "groups.available") return page([{ ...group, id: "g2", name: "New Codex", configured: false, models: [] }]);
    if (operation === "catalog") return { platforms: ["codex", "claude"], platform:"codex",groupId:group.id,name:group.name,models:["gpt-test"],availableAccountCount:2,accounts:[] };
    if (operation === "users.list") return page([user]);
    if (operation === "recharges.list") return page([recharge]);
    if (operation === "statistics") {
      const row = { period: new Date().toISOString().slice(0,10), rechargeCnyFen: 7000, rechargeCreditMicros: 10000000, manualCreditMicros: 0, rewardMicros: 0, subscriptionSalesMicros: 0, walletUsageMicros: 1000000, subscriptionUsageMicros: 500000, requestCount: 10, newUsers: 1, quotaMicros: 1000000 };
      return { items: [row], totals: row, current: { activeSubscriptions: 2, subscribedUsers: 1, todayUsedMicros: 500000, todayQuotaMicros: 1000000, expiringSubscriptions: 0, pendingRewards: 0 } };
    }
    if (operation === "overview") return { userCount: 1, groupCount: 1, pendingRechargeCount: 1, pendingReviewCount: 0, pendingReviewMicros: 0, balanceMicros: 10000000, frozenMicros: 0, debtMicros: 0 };
    if (operation === "codes.create") return { codes: ["redeem-one-time-code"] };
    return page();
  });
});
afterEach(cleanup);

describe("SaaS administrator", () => {
  it("shows subscription utilization and keeps unconfigured cost estimates blank", async () => {
    render(<SaasAdmin />);
    expect(await screen.findByText(/Today's quota utilization/)).toHaveTextContent("50.0%");
    const ratio = screen.getByLabelText(/upstream cost.*ratio/i);
    const exchange = screen.getByLabelText(/cost exchange rate/i);
    const fixed = screen.getByLabelText(/fixed expenses/i);
    expect(ratio).toHaveValue(null);
    await userEvent.type(ratio, "0.3");
    await userEvent.type(exchange, "7");
    await userEvent.type(fixed, "10");
    expect(screen.getByText("¥13.15")).toBeInTheDocument();
    expect(screen.getByText("¥56.85")).toBeInTheDocument();
  });

  it("hides the unreconciled request to-do from the operations dashboard", async () => {
    const original = invoke.getMockImplementation()!;
    invoke.mockImplementation(async (command, request) => {
      if (request?.operation === "overview") return { userCount: 1, groupCount: 1, pendingRechargeCount: 1, pendingReviewCount: 3, pendingReviewMicros: 3000000, balanceMicros: 10000000, frozenMicros: 0, debtMicros: 0 };
      return original(command, request);
    });
    render(<SaasAdmin />);
    expect(await screen.findByText("Pending recharges")).toBeInTheDocument();
    expect(screen.queryByText("Unreconciled requests")).not.toBeInTheDocument();
    expect(screen.getByText("Pending reconciliation")).toBeInTheDocument();
  });

  it("updates an existing user's email and password from the administrator panel", async () => {
    const original = invoke.getMockImplementation()!;
    invoke.mockImplementation(async (command, request) => {
      if (request?.operation === "users.list") return page([{ ...user, email: "member@example.com" }]);
      return original(command, request);
    });
    render(<SaasAdmin />);
    await userEvent.click(screen.getByRole("button", { name: /^users$/i }));
    await userEvent.click(await screen.findByRole("button", { name: /^edit user$/i }));
    await userEvent.clear(screen.getByLabelText(/^email$/i));
    await userEvent.type(screen.getByLabelText(/^email$/i), "updated@example.com");
    await userEvent.type(screen.getByLabelText(/^new password$/i), "new-secret-pass");
    await userEvent.click(screen.getByRole("button", { name: /save user/i }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("saas_admin", {
      operation: "users.update",
      payload: { userId: user.id, email: "updated@example.com", password: "new-secret-pass" },
    }));
  });
  it("grants and cancels a user's subscription with confirmation", async () => {
    const original = invoke.getMockImplementation()!;
    const subscription = { id: "sub1", userId: user.id, planId: "plan1", planName: "Monthly", status: "active", source: "administrator", dailyQuotaMicros: 1000000, todayUsedMicros: 0, todayFrozenMicros: 0, startsAt: "2026-01-01T00:00:00Z", expiresAt: "2099-01-01T00:00:00Z" };
    invoke.mockImplementation(async (command, request) => {
      if (request?.operation === "subscriptions.list") return page([subscription]);
      if (request?.operation === "subscriptions.plans.list") return page([{ id: "plan1", name: "Monthly", status: "active", dailyQuotaMicros: 1000000, durationDays: 30 }]);
      return original(command, request);
    });
    render(<SaasAdmin />);
    await userEvent.click(screen.getByRole("button", { name: /^users$/i }));
    await userEvent.click(await screen.findByRole("button", { name: /manage subscriptions/i }));
    await userEvent.selectOptions(await screen.findByLabelText(/^subscription plan$/i), "plan1");
    await userEvent.click(screen.getByRole("button", { name: /^grant subscription$/i }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("saas_admin", { operation: "subscriptions.grant", payload: { userId: user.id, planId: "plan1" } }));
    await userEvent.click(await screen.findByRole("button", { name: /^cancel subscription$/i }));
    expect(invoke.mock.calls.some(([,request]) => request?.operation === "subscriptions.cancel")).toBe(false);
    await userEvent.type(screen.getByLabelText(/cancellation reason/i), "Requested by user");
    await userEvent.click(screen.getByRole("button", { name: /^confirm cancellation$/i }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("saas_admin", { operation: "subscriptions.cancel", payload: { userId: user.id, id: "sub1", reason: "Requested by user" } }));
  });
  it("opens subscription management for the selected user", async () => {
    render(<SaasAdmin />);
    await userEvent.click(screen.getByRole("button", { name: /^users$/i }));
    await userEvent.click(await screen.findByRole("button", { name: /manage subscriptions/i }));
    expect(await screen.findByRole("dialog", { name: /manage subscriptions/i })).toBeInTheDocument();
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("saas_admin", { operation: "subscriptions.list", payload: { userId: user.id } }));
  });
  it("offers daily and monthly operating history", async () => {
    render(<SaasAdmin />);
    await userEvent.click(await screen.findByRole("button", { name: /more.*recharges/i }));
    expect(await screen.findByRole("heading", { name: /operating history/i })).toBeInTheDocument();
    await userEvent.selectOptions(screen.getByLabelText(/aggregation/i), "month");
    await userEvent.click(screen.getByRole("button", { name: /^query$/i }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("saas_admin", expect.objectContaining({ operation: "statistics", payload: expect.objectContaining({ granularity: "month" }) })));
  });
  it("shows only the plugin enable switch in platform settings", async () => {
    render(<SaasSettings />);
    expect(await screen.findByRole("checkbox", { name: /enable saas plugin|启用 SaaS 插件/i })).not.toBeChecked();
    expect(screen.queryByLabelText(/^github client secret$/i)).not.toBeInTheDocument();
    expect(screen.queryByLabelText(/public site url/i)).not.toBeInTheDocument();
    expect(screen.queryByLabelText(/email and password sign-in/i)).not.toBeInTheDocument();
  });

  it("keeps service configuration inside the SaaS administration panel", async () => {
    const actor = userEvent.setup();
    render(<SaasAdmin />);
    await actor.click(await screen.findByRole("button", { name: /^configuration$/i }));
    const input = await screen.findByLabelText(/public site url/i);
    expect(input).toHaveValue(config.publicBaseUrl);
    expect(screen.getByText("http://127.0.0.1:10086")).toBeInTheDocument();
    await actor.click(screen.getByRole("button", { name: /use current url/i }));
    expect(input).toHaveValue("http://127.0.0.1:10086");
  });

  it("configures whether email and password sign-in is available", async () => {
    const actor = userEvent.setup();
    render(<SaasAdmin />);
    await actor.click(await screen.findByRole("button", { name: /^configuration$/i }));
    const toggle = await screen.findByRole("checkbox", { name: /enable email and password sign-in/i });
    expect(toggle).toBeChecked();
    await actor.click(toggle);
    await actor.click(screen.getByRole("button", { name: /save configuration/i }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("saas_admin", expect.objectContaining({
      operation: "config.save",
      payload: expect.objectContaining({ passwordLoginEnabled: false }),
    })));
  });

  it("shows the homepage action in the SaaS panel header", async () => {
    render(<SaasAdmin />);
    expect(await screen.findByRole("button", { name: /open homepage/i })).toBeDisabled();
  });

  it("unlocks and enables the SaaS plugin from platform settings", async () => {
    const actor = userEvent.setup();
    const changed = vi.fn();
    render(<SaasSettings onConfigChanged={changed} />);
    await actor.click(await screen.findByRole("checkbox", { name: /enable saas plugin|启用 SaaS 插件/i }));
    expect(screen.getByRole("dialog", { name: /unlock saas/i })).toBeInTheDocument();
    const code = screen.getByLabelText(/beta access code/i);
    expect(code).toBeRequired();
    expect(screen.getByRole("link", { name: /join qq group/i })).toHaveAttribute("href", "https://qm.qq.com/q/eLyRbXjRcI");
    await actor.type(code, "ai-switch-ok");
    await actor.click(screen.getByRole("button", { name: /^unlock$/i }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("saas_admin", expect.objectContaining({
      operation: "activation.unlock",
      payload: { activationCode: "ai-switch-ok" },
    })));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("saas_admin", expect.objectContaining({
      operation: "config.save",
      payload: { enabled: true },
    })));
    expect(changed).toHaveBeenCalledOnce();
    expect(screen.queryByRole("dialog", { name: /unlock saas/i })).not.toBeInTheDocument();
  });

  it("creates an email and password user from the administrator panel", async () => {
    const actor = userEvent.setup();
    render(<SaasAdmin />);
    await actor.click(screen.getByRole("button", { name: /^users$/i }));
    await actor.click(await screen.findByRole("button", { name: /create account/i }));
    await actor.type(screen.getByLabelText(/^email$/i), "member@example.com");
    await actor.type(screen.getByLabelText(/initial password/i), "secret-pass");
    await actor.click(screen.getAllByRole("button", { name: /^create account$/i }).at(-1)!);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("saas_admin", {
      operation: "users.create",
      payload: { email: "member@example.com", password: "secret-pass" },
    }));
  });

  it("credits a user balance directly from the administrator panel", async () => {
    const actor = userEvent.setup();
    render(<SaasAdmin />);
    await actor.click(screen.getByRole("button", { name: /^users$/i }));
    await actor.click(await screen.findByRole("button", { name: /^credit$/i }));
    await actor.type(screen.getByLabelText(/credit amount/i), "12.50");
    await actor.type(screen.getByLabelText(/credit reason/i), "Manual service credit");
    await actor.click(screen.getByRole("button", { name: /confirm credit/i }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("saas_admin", {
      operation: "users.credit",
      payload: { userId: user.id, amountMicros: 12500000, reason: "Manual service credit" },
    }));
  });

  it("omits untouched secrets and calls the host only after a successful save", async () => {
    const changed = vi.fn();
    const actor = userEvent.setup();
    render(<SaasAdmin onConfigChanged={changed} />);
    await actor.click(await screen.findByRole("button", { name: /^configuration$/i }));
    await actor.click(await screen.findByRole("button", { name: /save configuration/i }));
    await waitFor(() => expect(changed).toHaveBeenCalledOnce());
    const saved = invoke.mock.calls.find(([, request]) => request?.operation === "config.save")?.[1].payload;
    expect(JSON.stringify(saved)).not.toContain("githubClientSecret\"");
    expect(JSON.stringify(saved)).not.toContain("postgresUrl\"");
  });

  it("edits only SaaS model pricing extensions of existing core groups", async () => {
    const actor = userEvent.setup();
    render(<SaasAdmin />);
    await actor.click(screen.getByRole("button", { name: /^groups & pricing$/i }));
    await actor.click(await screen.findByRole("button", { name: /^edit saas settings$/i }));
    expect(screen.queryByRole("button",{name:/new group/i})).not.toBeInTheDocument();
    expect(screen.queryByRole("button",{name:/delete/i})).not.toBeInTheDocument();
    expect(screen.getByLabelText(/agent group/i)).toHaveAttribute("readonly");
    expect(await screen.findByRole("checkbox",{name:"gpt-test"})).toBeChecked();
    expect(screen.getByLabelText(/input price/i)).toHaveAttribute("required");
    expect(screen.getByLabelText(/cache price/i)).toHaveAttribute("required");
    expect(screen.getByLabelText(/output price/i)).toHaveAttribute("required");
    await actor.click(screen.getByRole("button",{name:/save settings/i}));
    await waitFor(()=>expect(invoke).toHaveBeenCalledWith("saas_admin",{operation:"groups.save",payload:expect.objectContaining({id:group.id,models:group.models})}));
    const payload = invoke.mock.calls.find(([,request])=>request?.operation==="groups.save")?.[1].payload;
    expect(payload).not.toHaveProperty("name");
    expect(payload).not.toHaveProperty("batchIds");
    expect(payload).not.toHaveProperty("platform");
  });

  it("adds an unconfigured core group before it appears in pricing", async () => {
    const actor = userEvent.setup();
    render(<SaasAdmin />);
    await actor.click(screen.getByRole("button", { name: /^groups & pricing$/i }));
    await actor.click(await screen.findByRole("button", { name: /^add group$/i }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("saas_admin", { operation: "groups.available", payload: { page: 1, pageSize: 200 } }));
    await actor.selectOptions(screen.getByLabelText(/unconfigured agent group/i), "g2");
    await actor.click(screen.getByRole("button", { name: /configure pricing/i }));
    expect(await screen.findByRole("dialog", { name: /add saas group/i })).toBeInTheDocument();
    expect(screen.getByLabelText(/agent group/i)).toHaveValue("New Codex · codex");
  });

  it("shows both currencies and rate snapshot before approving a recharge", async () => {
    const actor = userEvent.setup();
    render(<SaasAdmin />);
    await actor.click(screen.getByRole("button", { name: /^recharge approval$/i }));
    await actor.click(await screen.findByRole("button", { name: /^review$/i }));
    const dialog = screen.getByRole("dialog");
    expect(dialog).toHaveTextContent("¥70.00");
    expect(dialog).toHaveTextContent("$10.00");
    expect(dialog).toHaveTextContent("octocat");
    await actor.type(screen.getByLabelText(/payment evidence/i),"Receipt verified");
    await actor.click(screen.getByRole("button", { name: /approve.*credit/i }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("saas_admin", { operation: "recharges.review", payload: expect.objectContaining({ id: "r1", status: "approved",reason:"Receipt verified" }) }));
  });
});
