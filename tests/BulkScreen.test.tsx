import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  createBulkOperation,
  createItemTag,
  createPluginLink,
  createTag,
  listBulkOperations,
  listItemTags,
  listPluginLinks,
  listTags,
  setPluginLinkEnabled,
} from "../src/lib/api/client";
import { BulkScreen } from "../src/screens/BulkScreen";
import {
  bulkOperationsFixture,
  itemTagsFixture,
  pluginLinksFixture,
  tagsFixture,
} from "../src/test/fixtures";

vi.mock("../src/lib/api/client", () => ({
  createBulkOperation: vi.fn(),
  createItemTag: vi.fn(),
  createPluginLink: vi.fn(),
  createTag: vi.fn(),
  listBulkOperations: vi.fn(),
  listItemTags: vi.fn(),
  listPluginLinks: vi.fn(),
  listTags: vi.fn(),
  setPluginLinkEnabled: vi.fn(),
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
      <BulkScreen />
    </QueryClientProvider>,
  );
}

function mockLists() {
  vi.mocked(listTags).mockResolvedValue(tagsFixture);
  vi.mocked(listItemTags).mockResolvedValue(itemTagsFixture);
  vi.mocked(listPluginLinks).mockResolvedValue(pluginLinksFixture);
  vi.mocked(listBulkOperations).mockResolvedValue(bulkOperationsFixture);
}

describe("BulkScreen", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("lists tags, assignments, plugin links, and bulk operations", async () => {
    mockLists();

    renderWithClient();

    expect(await screen.findByText("review")).toBeInTheDocument();
    expect(screen.getByText("Review bridge")).toBeInTheDocument();
    expect(screen.getByText("Apply review tag")).toBeInTheDocument();
    expect(screen.getAllByText("provider-1")).toHaveLength(2);
  });

  it("creates a tag", async () => {
    const user = userEvent.setup();
    mockLists();
    vi.mocked(createTag).mockResolvedValueOnce(tagsFixture[0]);

    renderWithClient();

    await screen.findByRole("button", { name: "Create tag" });
    await user.type(screen.getByLabelText("Tag name"), "review");
    await user.type(screen.getByLabelText("Tag description"), "Shared review items");
    await user.click(screen.getByRole("button", { name: "Create tag" }));

    await waitFor(() => expect(createTag).toHaveBeenCalled());
    expect(vi.mocked(createTag).mock.calls[0]?.[0]).toEqual({
      name: "review",
      color: "#3f6f5f",
      description: "Shared review items",
    });
  });

  it("assigns a tag to an item", async () => {
    const user = userEvent.setup();
    mockLists();
    vi.mocked(createItemTag).mockResolvedValueOnce(itemTagsFixture[0]);

    renderWithClient();

    await screen.findByRole("button", { name: "Assign tag" });
    await user.type(screen.getByLabelText("Assignment tag ID"), "tag-1");
    await user.type(screen.getByLabelText("Assignment item ID"), "provider-1");
    await user.click(screen.getByRole("button", { name: "Assign tag" }));

    await waitFor(() => expect(createItemTag).toHaveBeenCalled());
    expect(vi.mocked(createItemTag).mock.calls[0]?.[0]).toEqual({
      tag_id: "tag-1",
      item_type: "provider",
      item_id: "provider-1",
    });
  });

  it("creates a plugin link", async () => {
    const user = userEvent.setup();
    mockLists();
    vi.mocked(createPluginLink).mockResolvedValueOnce(pluginLinksFixture[0]);

    renderWithClient();

    await screen.findByRole("button", { name: "Create plugin link" });
    await user.type(screen.getByLabelText("Plugin link name"), "Review bridge");
    await user.type(screen.getByLabelText("Plugin item ID"), "provider-1");
    await user.type(screen.getByLabelText("Plugin notes"), "Metadata only");
    await user.click(screen.getByRole("button", { name: "Create plugin link" }));

    await waitFor(() => expect(createPluginLink).toHaveBeenCalled());
    expect(vi.mocked(createPluginLink).mock.calls[0]?.[0]).toEqual({
      name: "Review bridge",
      plugin_key: "review.bridge",
      item_type: "provider",
      item_id: "provider-1",
      config_json: "{\"mode\":\"metadata\"}",
      enabled: true,
      status: "configured",
      notes: "Metadata only",
    });
  });

  it("rejects invalid plugin config JSON before creating", async () => {
    const user = userEvent.setup();
    mockLists();

    renderWithClient();

    await screen.findByRole("button", { name: "Create plugin link" });
    fireEvent.change(screen.getByLabelText("Plugin config JSON"), {
      target: { value: "[]" },
    });
    await user.click(screen.getByRole("button", { name: "Create plugin link" }));

    expect(await screen.findByText("Plugin config JSON must be an object.")).toBeInTheDocument();
    expect(createPluginLink).not.toHaveBeenCalled();
  });

  it("creates a bulk operation record", async () => {
    const user = userEvent.setup();
    mockLists();
    vi.mocked(createBulkOperation).mockResolvedValueOnce(bulkOperationsFixture[0]);

    renderWithClient();

    await screen.findByRole("button", { name: "Create bulk operation" });
    await user.type(screen.getByLabelText("Bulk operation name"), "Apply review tag");
    fireEvent.change(screen.getByLabelText("Bulk parameters JSON"), {
      target: { value: "{\"tag_id\":\"tag-1\"}" },
    });
    await user.click(screen.getByRole("button", { name: "Create bulk operation" }));

    await waitFor(() => expect(createBulkOperation).toHaveBeenCalled());
    expect(vi.mocked(createBulkOperation).mock.calls[0]?.[0]).toEqual({
      name: "Apply review tag",
      operation_type: "tag_apply",
      target_type: "provider",
      item_ids_json: "[\"provider-1\"]",
      parameters_json: "{\"tag_id\":\"tag-1\"}",
      dry_run: true,
      status: "planned",
      summary_json: "{}",
    });
  });

  it("toggles plugin links", async () => {
    const user = userEvent.setup();
    mockLists();
    vi.mocked(setPluginLinkEnabled).mockResolvedValueOnce({
      ...pluginLinksFixture[0],
      enabled: 0,
      status: "paused",
    });

    renderWithClient();

    await user.click(await screen.findByRole("button", { name: "Disable Review bridge" }));

    expect(vi.mocked(setPluginLinkEnabled).mock.calls[0]?.[0]).toEqual({
      id: "plugin-link-1",
      enabled: false,
    });
  });
});
