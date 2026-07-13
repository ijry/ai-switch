import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createMcpServer, listMcpServers, setMcpServerEnabled } from "../src/lib/api/client";
import { McpScreen } from "../src/screens/McpScreen";
import { mcpServersFixture } from "../src/test/fixtures";

vi.mock("../src/lib/api/client", () => ({
  createMcpServer: vi.fn(),
  listMcpServers: vi.fn(),
  setMcpServerEnabled: vi.fn(),
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
      <McpScreen />
    </QueryClientProvider>,
  );
}

describe("McpScreen", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("lists MCP servers", async () => {
    vi.mocked(listMcpServers).mockResolvedValueOnce(mcpServersFixture);

    renderWithClient();

    expect(await screen.findByText("Filesystem")).toBeInTheDocument();
    expect(screen.getByText("stdio - npx")).toBeInTheDocument();
    expect(screen.getByText("Local project files")).toBeInTheDocument();
    expect(screen.getByText("disabled")).toBeInTheDocument();
  });

  it("creates a stdio MCP server", async () => {
    const user = userEvent.setup();
    vi.mocked(listMcpServers).mockResolvedValue([]);
    vi.mocked(createMcpServer).mockResolvedValueOnce(mcpServersFixture[0]);

    renderWithClient();

    await screen.findByRole("button", { name: "Create MCP server" });
    await user.type(screen.getByLabelText("Name"), "Filesystem");
    await user.type(screen.getByLabelText("Command"), "npx");
    fireEvent.change(screen.getByLabelText("Args JSON"), {
      target: { value: "[\"-y\",\"@modelcontextprotocol/server-filesystem\"]" },
    });
    fireEvent.change(screen.getByLabelText("Environment JSON"), {
      target: { value: "{\"BRAVE_API_KEY\":\"env://BRAVE_API_KEY\"}" },
    });
    await user.type(screen.getByLabelText("Notes"), "Local files");
    await user.click(screen.getByRole("button", { name: "Create MCP server" }));

    await waitFor(() => expect(createMcpServer).toHaveBeenCalled());
    expect(vi.mocked(createMcpServer).mock.calls[0]?.[0]).toEqual({
      name: "Filesystem",
      transport: "stdio",
      command: "npx",
      args_json: "[\"-y\",\"@modelcontextprotocol/server-filesystem\"]",
      url: null,
      env_json: "{\"BRAVE_API_KEY\":\"env://BRAVE_API_KEY\"}",
      enabled: true,
      notes: "Local files",
    });
  });

  it("rejects invalid environment JSON before creating", async () => {
    const user = userEvent.setup();
    vi.mocked(listMcpServers).mockResolvedValue([]);

    renderWithClient();

    await screen.findByRole("button", { name: "Create MCP server" });
    await user.type(screen.getByLabelText("Name"), "Broken MCP");
    await user.type(screen.getByLabelText("Command"), "node");
    fireEvent.change(screen.getByLabelText("Environment JSON"), {
      target: { value: "[" },
    });
    await user.click(screen.getByRole("button", { name: "Create MCP server" }));

    expect(await screen.findByText("MCP environment JSON must be valid JSON.")).toBeInTheDocument();
    expect(createMcpServer).not.toHaveBeenCalled();
  });

  it("toggles MCP server enabled state", async () => {
    const user = userEvent.setup();
    vi.mocked(listMcpServers).mockResolvedValue(mcpServersFixture);
    vi.mocked(setMcpServerEnabled).mockResolvedValueOnce({
      ...mcpServersFixture[0],
      enabled: 0,
    });

    renderWithClient();

    await user.click(await screen.findByRole("button", { name: "Disable Filesystem" }));

    expect(vi.mocked(setMcpServerEnabled).mock.calls[0]?.[0]).toEqual({
      id: "mcp-1",
      enabled: false,
    });
  });
});
