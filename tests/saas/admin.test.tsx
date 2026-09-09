import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { SaasAdmin, SaasSettings } from "../../src/saas";
import { config, group, page, recharge, user } from "./fixtures";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("../../src/lib/transport", () => ({ getTransport: () => ({ call: invoke }), isDesktop: () => true }));
beforeEach(() => {
  localStorage.setItem("saas.locale", "en");
  invoke.mockReset().mockImplementation(async (_command: string, { operation }: { operation: string }) => {
    if (_command === "get_web_server_status") return { running: true, host: "127.0.0.1", port: 10086, baseUrl: "http://127.0.0.1:10086" };
    if (operation === "config.get" || operation === "config.save") return config;
    if (operation === "activation.status") return { unlocked: false };
    if (operation === "activation.unlock") return { unlocked: true };
    if (operation === "groups.list") return page([group]);
    if (operation === "groups.available") return page([{ ...group, id: "g2", name: "New Codex", configured: false, models: [] }]);
    if (operation === "catalog") return { platforms: ["codex", "claude"], platform:"codex",groupId:group.id,name:group.name,models:["gpt-test"],availableAccountCount:2,accounts:[] };
    if (operation === "users.list") return page([user]);
    if (operation === "recharges.list") return page([recharge]);
    if (operation === "overview") return { users: 1, groups: 1, pendingRecharges: 1, pendingReconciliations: 0, totalBalanceMicros: 10000000, todayCostMicros: 1000 };
    if (operation === "codes.create") return { codes: ["redeem-one-time-code"] };
    return page();
  });
});
afterEach(cleanup);

describe("SaaS administrator", () => {
  it("loads the disabled configuration without echoing stored secrets", async () => {
    render(<SaasSettings />);
    expect(await screen.findByRole("checkbox", { name: /enable saas/i })).not.toBeChecked();
    expect(screen.getByLabelText(/^github client secret$/i)).toHaveValue("");
    expect(screen.getByText(/already configured/i)).toBeInTheDocument();
  });

  it("shows the homepage action in the SaaS panel header", async () => {
    render(<SaasAdmin />);
    expect(await screen.findByRole("button", { name: /open homepage/i })).toBeDisabled();
  });

  it("unlocks SaaS from a beta-code dialog without enabling the public site", async () => {
    const actor = userEvent.setup();
    const changed = vi.fn();
    render(<SaasSettings onConfigChanged={changed} />);
    await actor.click(await screen.findByRole("checkbox", { name: /enable saas/i }));
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
    expect(changed).toHaveBeenCalledOnce();
    expect(screen.queryByRole("dialog", { name: /unlock saas/i })).not.toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: /enable saas/i })).not.toBeChecked();
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
    render(<SaasSettings onConfigChanged={changed} />);
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
