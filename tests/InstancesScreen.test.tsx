import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createInstance, listInstances, setInstanceStatus } from "../src/lib/api/client";
import { InstancesScreen } from "../src/screens/InstancesScreen";
import { managedInstancesFixture } from "../src/test/fixtures";

vi.mock("../src/lib/api/client", () => ({
  createInstance: vi.fn(),
  listInstances: vi.fn(),
  setInstanceStatus: vi.fn(),
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
      <InstancesScreen />
    </QueryClientProvider>,
  );
}

describe("InstancesScreen", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("lists managed instances", async () => {
    vi.mocked(listInstances).mockResolvedValueOnce(managedInstancesFixture);

    renderWithClient();

    expect(await screen.findByText("Codex Review")).toBeInTheDocument();
    expect(screen.getByText("Local metadata only")).toBeInTheDocument();
    expect(screen.getAllByText("[\"--profile\",\"review\"]")).toHaveLength(2);
  });

  it("creates a managed instance", async () => {
    const user = userEvent.setup();
    vi.mocked(listInstances).mockResolvedValue([]);
    vi.mocked(createInstance).mockResolvedValueOnce(managedInstancesFixture[0]);

    renderWithClient();

    await screen.findByRole("button", { name: "Create instance" });
    await user.type(screen.getByLabelText("Instance name"), "Codex Review");
    await user.type(screen.getByLabelText("Instance target app ID"), "target-codex");
    await user.type(screen.getByLabelText("Instance provider ID"), "provider-1");
    fireEvent.change(screen.getByLabelText("Instance env JSON"), {
      target: { value: "{\"API_KEY\":\"env://API_KEY\"}" },
    });
    fireEvent.change(screen.getByLabelText("Instance profile JSON"), {
      target: { value: "{\"workspace\":\"review\"}" },
    });
    await user.type(screen.getByLabelText("Instance notes"), "Local metadata only");
    await user.click(screen.getByRole("button", { name: "Create instance" }));

    await waitFor(() => expect(createInstance).toHaveBeenCalled());
    expect(vi.mocked(createInstance).mock.calls[0]?.[0]).toEqual({
      name: "Codex Review",
      target_app_id: "target-codex",
      provider_id: "provider-1",
      launch_args_json: "[\"--profile\",\"review\"]",
      env_json: "{\"API_KEY\":\"env://API_KEY\"}",
      profile_json: "{\"workspace\":\"review\"}",
      status: "configured",
      notes: "Local metadata only",
    });
  });

  it("rejects invalid launch args JSON before creating", async () => {
    const user = userEvent.setup();
    vi.mocked(listInstances).mockResolvedValue([]);

    renderWithClient();

    await screen.findByRole("button", { name: "Create instance" });
    fireEvent.change(screen.getByLabelText("Launch args JSON"), {
      target: { value: "{\"bad\":true}" },
    });
    await user.click(screen.getByRole("button", { name: "Create instance" }));

    expect(
      await screen.findByText("Launch args JSON must be an array of strings."),
    ).toBeInTheDocument();
    expect(createInstance).not.toHaveBeenCalled();
  });

  it("records instance status changes", async () => {
    const user = userEvent.setup();
    vi.mocked(listInstances).mockResolvedValue(managedInstancesFixture);
    vi.mocked(setInstanceStatus).mockResolvedValueOnce({
      ...managedInstancesFixture[0],
      status: "running",
    });

    renderWithClient();

    await user.click(await screen.findByRole("button", { name: "Mark Codex Review running" }));

    expect(vi.mocked(setInstanceStatus).mock.calls[0]?.[0]).toEqual({
      id: "instance-1",
      status: "running",
    });
  });
});
