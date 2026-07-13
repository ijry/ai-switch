import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  createFailoverPolicy,
  createProxyProfile,
  createUsageEvent,
  listFailoverPolicies,
  listProxyProfiles,
  listUsageEvents,
} from "../src/lib/api/client";
import { RoutingScreen } from "../src/screens/RoutingScreen";
import {
  failoverPoliciesFixture,
  proxyProfilesFixture,
  usageEventsFixture,
} from "../src/test/fixtures";

vi.mock("../src/lib/api/client", () => ({
  createFailoverPolicy: vi.fn(),
  createProxyProfile: vi.fn(),
  createUsageEvent: vi.fn(),
  listFailoverPolicies: vi.fn(),
  listProxyProfiles: vi.fn(),
  listUsageEvents: vi.fn(),
}));

function renderWithClient() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });

  return render(
    <QueryClientProvider client={queryClient}>
      <RoutingScreen />
    </QueryClientProvider>,
  );
}

function mockEmptyLists() {
  vi.mocked(listProxyProfiles).mockResolvedValue([]);
  vi.mocked(listFailoverPolicies).mockResolvedValue([]);
  vi.mocked(listUsageEvents).mockResolvedValue([]);
}

describe("RoutingScreen", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("lists routing records", async () => {
    vi.mocked(listProxyProfiles).mockResolvedValueOnce(proxyProfilesFixture);
    vi.mocked(listFailoverPolicies).mockResolvedValueOnce(failoverPoliciesFixture);
    vi.mocked(listUsageEvents).mockResolvedValueOnce(usageEventsFixture);

    renderWithClient();

    expect(await screen.findByText("Local Proxy")).toBeInTheDocument();
    expect(screen.getByText("Primary then backup")).toBeInTheDocument();
    expect(screen.getByText("12 count request")).toBeInTheDocument();
  });

  it("creates a proxy profile", async () => {
    const user = userEvent.setup();
    mockEmptyLists();
    vi.mocked(createProxyProfile).mockResolvedValueOnce(proxyProfilesFixture[0]);

    renderWithClient();

    await screen.findByRole("button", { name: "Create proxy profile" });
    await user.type(screen.getByLabelText("Proxy name"), "Local Proxy");
    await user.type(screen.getByLabelText("Proxy auth ref"), "env://LOCAL_PROXY_AUTH");
    await user.type(screen.getByLabelText("Proxy notes"), "Local proxy metadata");
    await user.click(screen.getByRole("button", { name: "Create proxy profile" }));

    await waitFor(() => expect(createProxyProfile).toHaveBeenCalled());
    expect(vi.mocked(createProxyProfile).mock.calls[0]?.[0]).toEqual({
      name: "Local Proxy",
      endpoint_url: "http://127.0.0.1:7890",
      auth_ref: "env://LOCAL_PROXY_AUTH",
      enabled: true,
      notes: "Local proxy metadata",
    });
  });

  it("creates a failover policy", async () => {
    const user = userEvent.setup();
    mockEmptyLists();
    vi.mocked(createFailoverPolicy).mockResolvedValueOnce(failoverPoliciesFixture[0]);

    renderWithClient();

    await screen.findByRole("button", { name: "Create failover policy" });
    await user.type(screen.getByLabelText("Failover name"), "Primary then backup");
    await user.click(screen.getByRole("button", { name: "Create failover policy" }));

    await waitFor(() => expect(createFailoverPolicy).toHaveBeenCalled());
    expect(vi.mocked(createFailoverPolicy).mock.calls[0]?.[0]).toEqual({
      name: "Primary then backup",
      strategy: "ordered",
      provider_ids_json: "[\"provider-1\",\"provider-2\"]",
      enabled: true,
      notes: null,
    });
  });

  it("rejects invalid usage metadata before recording", async () => {
    const user = userEvent.setup();
    mockEmptyLists();

    renderWithClient();

    await screen.findByRole("button", { name: "Record usage event" });
    fireEvent.change(screen.getByLabelText("Usage metadata JSON"), {
      target: { value: "[" },
    });
    await user.click(screen.getByRole("button", { name: "Record usage event" }));

    expect(await screen.findByText("Usage metadata JSON must be valid JSON.")).toBeInTheDocument();
    expect(createUsageEvent).not.toHaveBeenCalled();
  });

  it("records a manual usage event", async () => {
    const user = userEvent.setup();
    mockEmptyLists();
    vi.mocked(createUsageEvent).mockResolvedValueOnce(usageEventsFixture[0]);

    renderWithClient();

    await screen.findByRole("button", { name: "Record usage event" });
    await user.type(screen.getByLabelText("Usage provider ID"), "provider-1");
    await user.click(screen.getByRole("button", { name: "Record usage event" }));

    await waitFor(() => expect(createUsageEvent).toHaveBeenCalled());
    expect(vi.mocked(createUsageEvent).mock.calls[0]?.[0]).toEqual({
      provider_id: "provider-1",
      official_account_id: null,
      source_label: "manual",
      metric_type: "request",
      amount: 1,
      unit: "count",
      metadata_json: "{}",
    });
  });
});
