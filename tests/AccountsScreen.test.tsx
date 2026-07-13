import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  createOfficialAccount,
  listBatchGroups,
  listOfficialAccountStatuses,
  refreshOfficialAccountQuotaSnapshot,
  recordOfficialAccountQuotaSnapshot,
} from "../src/lib/api/client";
import { AccountsScreen } from "../src/screens/AccountsScreen";
import {
  batchGroupsFixture,
  officialAccountsFixture,
  officialAccountStatusesFixture,
} from "../src/test/fixtures";

vi.mock("../src/lib/api/client", () => ({
  createOfficialAccount: vi.fn(),
  listBatchGroups: vi.fn(),
  listOfficialAccountStatuses: vi.fn(),
  refreshOfficialAccountQuotaSnapshot: vi.fn(),
  recordOfficialAccountQuotaSnapshot: vi.fn(),
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
      <AccountsScreen />
    </QueryClientProvider>,
  );
}

describe("AccountsScreen", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("lists metadata-only official accounts", async () => {
    vi.mocked(listOfficialAccountStatuses).mockResolvedValueOnce(
      officialAccountStatusesFixture,
    );
    vi.mocked(listBatchGroups).mockResolvedValueOnce(batchGroupsFixture);

    renderWithClient();

    expect(await screen.findByText("Team Codex")).toBeInTheDocument();
    expect(screen.getByText("codex")).toBeInTheDocument();
    expect(screen.getByText("Email: team@example.com")).toBeInTheDocument();
    expect(screen.getByText("Secret ref: secret://account/team")).toBeInTheDocument();
    expect(screen.getByText("Quota: warning")).toBeInTheDocument();
    expect(screen.getByText("Remaining: 12% remaining")).toBeInTheDocument();
  });

  it("creates an official account and attaches it to a batch", async () => {
    const user = userEvent.setup();
    vi.mocked(listOfficialAccountStatuses).mockResolvedValue([]);
    vi.mocked(listBatchGroups).mockResolvedValue(batchGroupsFixture);
    vi.mocked(createOfficialAccount).mockResolvedValueOnce(officialAccountsFixture[0]);

    renderWithClient();

    await screen.findByText("Create official account");
    await user.type(screen.getByLabelText("Display name"), "Team Codex");
    await user.type(screen.getByLabelText("Email"), "team@example.com");
    await user.type(screen.getByLabelText("Plan"), "team");
    await user.type(screen.getByLabelText("Secret ref"), "secret://account/team");
    await user.selectOptions(screen.getByLabelText("Batch"), "batch-1");
    await user.click(screen.getByRole("button", { name: "Create account" }));

    await waitFor(() => expect(createOfficialAccount).toHaveBeenCalled());
    expect(vi.mocked(createOfficialAccount).mock.calls[0]?.[0]).toEqual({
      account: {
        platform: "codex",
        display_name: "Team Codex",
        email: "team@example.com",
        plan: "team",
        account_metadata_json: "{}",
        secret_ref: "secret://account/team",
      },
      batch_id: "batch-1",
    });
  });

  it("rejects invalid metadata JSON before creating", async () => {
    const user = userEvent.setup();
    vi.mocked(listOfficialAccountStatuses).mockResolvedValue([]);
    vi.mocked(listBatchGroups).mockResolvedValue([]);

    renderWithClient();

    await screen.findByText("Create official account");
    await user.type(screen.getByLabelText("Display name"), "Broken Account");
    fireEvent.change(screen.getByLabelText("Metadata JSON"), { target: { value: "{" } });
    await user.click(screen.getByRole("button", { name: "Create account" }));

    expect(await screen.findByText("Account metadata must be valid JSON.")).toBeInTheDocument();
    expect(createOfficialAccount).not.toHaveBeenCalled();
  });

  it("records a manual quota snapshot for an account", async () => {
    const user = userEvent.setup();
    vi.mocked(listOfficialAccountStatuses).mockResolvedValue(
      officialAccountStatusesFixture,
    );
    vi.mocked(listBatchGroups).mockResolvedValue(batchGroupsFixture);
    vi.mocked(recordOfficialAccountQuotaSnapshot).mockResolvedValueOnce({
      account: {
        ...officialAccountsFixture[0],
        quota_snapshot_id: "quota-2",
      },
      quota_snapshot: {
        id: "quota-2",
        owner_type: "official_account",
        owner_id: "account-1",
        status: "ok",
        remaining_label: "80% remaining",
        reset_at: "2026-07-15T00:00:00Z",
        summary_json: "{}",
        raw_excerpt_json: "{}",
        fetched_at: "2026-07-13T02:00:00Z",
      },
    });

    renderWithClient();

    await user.click(
      await screen.findByRole("button", { name: "Record quota for Team Codex" }),
    );
    await user.selectOptions(screen.getByLabelText("Quota status"), "ok");
    await user.clear(screen.getByLabelText("Remaining label"));
    await user.type(screen.getByLabelText("Remaining label"), "80% remaining");
    await user.type(screen.getByLabelText("Reset at"), "2026-07-15T00:00:00Z");
    await user.click(screen.getByRole("button", { name: "Save quota snapshot" }));

    expect(vi.mocked(recordOfficialAccountQuotaSnapshot).mock.calls[0]?.[0]).toEqual({
      account_id: "account-1",
      status: "ok",
      remaining_label: "80% remaining",
      reset_at: "2026-07-15T00:00:00Z",
      summary_json: "{}",
      raw_excerpt_json: "{}",
    });
  });

  it("refreshes quota for an account from configured metadata", async () => {
    const user = userEvent.setup();
    vi.mocked(listOfficialAccountStatuses).mockResolvedValue(
      officialAccountStatusesFixture,
    );
    vi.mocked(listBatchGroups).mockResolvedValue(batchGroupsFixture);
    vi.mocked(refreshOfficialAccountQuotaSnapshot).mockResolvedValueOnce({
      account: {
        ...officialAccountsFixture[0],
        quota_snapshot_id: "quota-2",
      },
      quota_snapshot: {
        id: "quota-2",
        owner_type: "official_account",
        owner_id: "account-1",
        status: "ok",
        remaining_label: "80% remaining",
        reset_at: null,
        summary_json: "{}",
        raw_excerpt_json: "{}",
        fetched_at: "2026-07-13T02:00:00Z",
      },
    });

    renderWithClient();

    await user.click(
      await screen.findByRole("button", { name: "Refresh quota for Team Codex" }),
    );

    expect(vi.mocked(refreshOfficialAccountQuotaSnapshot).mock.calls[0]?.[0]).toEqual({
      account_id: "account-1",
    });
  });
});
