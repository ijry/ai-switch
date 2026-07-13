import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import {
  createProviderFromPreset,
  exportExampleJson,
  importDeepLink,
  importOfficialAccountJson,
  importExampleJson,
  listProviderPresets,
  refreshTrayMenu,
} from "../src/lib/api/client";
import { ImportsScreen } from "../src/screens/ImportsScreen";
import { providerPresetsFixture, providersFixture } from "../src/test/fixtures";

vi.mock("../src/lib/api/client", () => ({
  createProviderFromPreset: vi.fn(),
  exportExampleJson: vi.fn(),
  importDeepLink: vi.fn(),
  importOfficialAccountJson: vi.fn(),
  importExampleJson: vi.fn(),
  listProviderPresets: vi.fn(),
  refreshTrayMenu: vi.fn(() =>
    Promise.resolve({ provider_count: 0, target_count: 0, switch_item_count: 0 }),
  ),
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
      <ImportsScreen />
    </QueryClientProvider>,
  );
}

describe("ImportsScreen", () => {
  it("creates a provider from a preset batch", async () => {
    vi.mocked(listProviderPresets).mockResolvedValueOnce(providerPresetsFixture);
    vi.mocked(createProviderFromPreset).mockResolvedValueOnce({
      provider: providersFixture[0],
      batch_id: "batch-1",
    });

    renderWithClient();

    expect(await screen.findByText("OpenAI Compatible")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Create OpenAI Compatible" }));

    await waitFor(() => {
      expect(createProviderFromPreset).toHaveBeenCalledWith({
        preset_id: "openai-compatible",
        batch_name: "Provider presets",
      });
    });
    expect(await screen.findByText("Created provider Acme Provider.")).toBeInTheDocument();
  });

  it("exports current data as example JSON", async () => {
    vi.mocked(listProviderPresets).mockResolvedValueOnce(providerPresetsFixture);
    vi.mocked(exportExampleJson).mockResolvedValueOnce({
      json: "{\"providers\":[],\"accounts\":[]}",
      provider_count: 0,
      account_count: 0,
    });

    renderWithClient();

    await screen.findByText("OpenAI Compatible");
    await userEvent.click(screen.getByRole("button", { name: "Export example JSON" }));

    await waitFor(() => {
      expect(exportExampleJson).toHaveBeenCalled();
    });
    expect(await screen.findByText("Exported 0 providers and 0 accounts.")).toBeInTheDocument();
    expect(screen.getByLabelText("Exported example JSON")).toHaveValue(
      "{\"providers\":[],\"accounts\":[]}",
    );
  });

  it("imports official account metadata bundles", async () => {
    vi.mocked(listProviderPresets).mockResolvedValueOnce(providerPresetsFixture);
    vi.mocked(importOfficialAccountJson).mockResolvedValueOnce({
      id: "job-1",
      source_type: "official_account_json",
      source_label: "manual account paste",
      batch_id: "batch-accounts",
      strategy: "skip",
      status: "completed",
      success_count: 1,
      failure_count: 0,
      conflict_count: 0,
      summary_json: "{}",
      created_at: "2026-07-13T00:00:00Z",
      completed_at: "2026-07-13T00:00:01Z",
    });

    renderWithClient();

    await screen.findByText("OpenAI Compatible");
    await userEvent.selectOptions(screen.getByLabelText("Account platform"), "cursor");
    await userEvent.click(screen.getByRole("button", { name: "Import official accounts" }));

    await waitFor(() => {
      expect(importOfficialAccountJson).toHaveBeenCalledWith({
        batch_name: "Official accounts",
        source_label: "manual account paste",
        platform: "cursor",
        json: expect.stringContaining("\"accounts\""),
      });
    });
    expect(
      await screen.findByText("Imported 1 official accounts into batch batch-accounts."),
    ).toBeInTheDocument();
  });

  it("imports pasted deep links", async () => {
    vi.mocked(listProviderPresets).mockResolvedValueOnce(providerPresetsFixture);
    vi.mocked(importDeepLink).mockResolvedValueOnce({
      id: "job-deep-link",
      source_type: "example_json",
      source_label: "shared",
      batch_id: "batch-deep-link",
      strategy: "skip",
      status: "completed",
      success_count: 2,
      failure_count: 0,
      conflict_count: 0,
      summary_json: "{}",
      created_at: "2026-07-13T00:00:00Z",
      completed_at: "2026-07-13T00:00:01Z",
    });
    const url =
      "ai-switch://import/example_json?batch_name=Shared&source_label=team&payload=eyJwcm92aWRlcnMiOltdfQ";

    renderWithClient();

    await screen.findByText("OpenAI Compatible");
    await userEvent.type(screen.getByLabelText("Deep-link URL"), url);
    await userEvent.click(screen.getByRole("button", { name: "Import deep link" }));

    await waitFor(() => {
      expect(importDeepLink).toHaveBeenCalledWith({ url });
    });
    expect(
      await screen.findByText("Imported 2 deep-link records into batch batch-deep-link."),
    ).toBeInTheDocument();
    expect(refreshTrayMenu).toHaveBeenCalled();
  });
});
