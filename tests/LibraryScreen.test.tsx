import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  createPromptAsset,
  listPromptAssets,
  setPromptAssetEnabled,
} from "../src/lib/api/client";
import { LibraryScreen } from "../src/screens/LibraryScreen";
import { promptAssetsFixture } from "../src/test/fixtures";

vi.mock("../src/lib/api/client", () => ({
  createPromptAsset: vi.fn(),
  listPromptAssets: vi.fn(),
  setPromptAssetEnabled: vi.fn(),
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
      <LibraryScreen />
    </QueryClientProvider>,
  );
}

describe("LibraryScreen", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("lists prompt and skill assets", async () => {
    vi.mocked(listPromptAssets).mockResolvedValueOnce(promptAssetsFixture);

    renderWithClient();

    expect(await screen.findByText("Review Prompt")).toBeInTheDocument();
    expect(screen.getByText("prompt - Find risky behavior changes.")).toBeInTheDocument();
    expect(screen.getByText("Release Notes")).toBeInTheDocument();
    expect(screen.getByText("skill - No description")).toBeInTheDocument();
  });

  it("creates a skill asset", async () => {
    const user = userEvent.setup();
    vi.mocked(listPromptAssets).mockResolvedValue([]);
    vi.mocked(createPromptAsset).mockResolvedValueOnce(promptAssetsFixture[1]);

    renderWithClient();

    await screen.findByRole("button", { name: "Create library item" });
    await user.selectOptions(screen.getByLabelText("Type"), "skill");
    await user.type(screen.getByLabelText("Name"), "Release Notes");
    await user.type(screen.getByLabelText("Description"), "Summarize releases.");
    await user.type(screen.getByLabelText("Body"), "Summarize merged changes.");
    fireEvent.change(screen.getByLabelText("Tags JSON"), {
      target: { value: "[\"release\"]" },
    });
    fireEvent.change(screen.getByLabelText("Metadata JSON"), {
      target: { value: "{\"owner\":\"docs\"}" },
    });
    await user.click(screen.getByRole("button", { name: "Create library item" }));

    await waitFor(() => expect(createPromptAsset).toHaveBeenCalled());
    expect(vi.mocked(createPromptAsset).mock.calls[0]?.[0]).toEqual({
      item_type: "skill",
      name: "Release Notes",
      description: "Summarize releases.",
      body: "Summarize merged changes.",
      tags_json: "[\"release\"]",
      metadata_json: "{\"owner\":\"docs\"}",
      enabled: true,
    });
  });

  it("rejects invalid tags JSON before creating", async () => {
    const user = userEvent.setup();
    vi.mocked(listPromptAssets).mockResolvedValue([]);

    renderWithClient();

    await screen.findByRole("button", { name: "Create library item" });
    await user.type(screen.getByLabelText("Name"), "Broken Prompt");
    await user.type(screen.getByLabelText("Body"), "Find regressions.");
    fireEvent.change(screen.getByLabelText("Tags JSON"), {
      target: { value: "{\"tag\":\"review\"}" },
    });
    await user.click(screen.getByRole("button", { name: "Create library item" }));

    expect(await screen.findByText("Tags JSON must be an array of strings.")).toBeInTheDocument();
    expect(createPromptAsset).not.toHaveBeenCalled();
  });

  it("toggles prompt asset enabled state", async () => {
    const user = userEvent.setup();
    vi.mocked(listPromptAssets).mockResolvedValue(promptAssetsFixture);
    vi.mocked(setPromptAssetEnabled).mockResolvedValueOnce({
      ...promptAssetsFixture[0],
      enabled: 0,
    });

    renderWithClient();

    await user.click(await screen.findByRole("button", { name: "Disable Review Prompt" }));

    expect(vi.mocked(setPromptAssetEnabled).mock.calls[0]?.[0]).toEqual({
      id: "prompt-1",
      enabled: false,
    });
  });
});
