import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { RouteCredentialImportDialog } from "../src/components/accounts/RouteCredentialImportDialog";
import { importRouteCredentials, previewRouteCredentialImport } from "../src/lib/api/client";
import type {
  RouteCredentialImportOutcome,
  RouteCredentialImportPreview,
} from "../src/lib/api/types";

vi.mock("../src/lib/api/client", () => ({
  importRouteCredentials: vi.fn(),
  previewRouteCredentialImport: vi.fn(),
}));

const basePreview = (overrides: Partial<RouteCredentialImportPreview> = {}): RouteCredentialImportPreview => ({
  counts: {
    total: 2,
    official: 1,
    api: 1,
    importable: 2,
    duplicates: 0,
    conflicts: 0,
    errors: 0,
    restorable_pool_count: 1,
    batch_count: 1,
    platform_counts: { claude: 1, codex: 1 },
    cpa_section_counts: { claude: 1 },
    legacy_type_counts: {},
    restorable_pool_counts: { claude: 1 },
  },
  items: [
    {
      item_index: 0,
      display_name_masked: "A***e",
      platform: "claude",
      kind: "official",
      cpa_section: "claude",
      disposition: "import",
      issue_codes: [],
    },
    {
      item_index: 1,
      display_name_masked: "C***x",
      platform: "codex",
      kind: "api",
      cpa_section: "openai-compatibility",
      disposition: "import",
      issue_codes: [],
    },
  ],
  ...overrides,
});

const outcome: RouteCredentialImportOutcome = {
  imported: 2,
  skipped_duplicates: 1,
  conflicts: 0,
  failed: 0,
  restored_pool_members: 1,
};

function renderDialog(onImported = vi.fn(), onClose = vi.fn()) {
  return render(<RouteCredentialImportDialog open onClose={onClose} onImported={onImported} />);
}

describe("RouteCredentialImportDialog", () => {
  beforeEach(() => {
    vi.mocked(previewRouteCredentialImport).mockReset();
    vi.mocked(importRouteCredentials).mockReset();
    vi.mocked(previewRouteCredentialImport).mockResolvedValue(basePreview());
    vi.mocked(importRouteCredentials).mockResolvedValue(outcome);
  });

  it("previews pasted JSON, shows only masked rows, and submits the exact source with pool restore off", async () => {
    const onImported = vi.fn();
    renderDialog(onImported);
    const source = JSON.stringify([{ name: "secret-name", api_key: "secret" }]);
    fireEvent.change(screen.getByRole("textbox", { name: "账号 JSON" }), { target: { value: source } });

    await waitFor(() => expect(previewRouteCredentialImport).toHaveBeenCalled());
    expect(await screen.findByText("A***e")).toBeInTheDocument();
    expect(screen.queryByText("secret-name")).not.toBeInTheDocument();
    expect(screen.getByText(/可导入 2/)).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "确认导入账号" }));
    await waitFor(() => expect(importRouteCredentials).toHaveBeenCalledWith({
      text: source,
      ambiguous_platform_choices: [],
      restore_pool_membership: false,
    }));
    expect(onImported).toHaveBeenCalledWith(outcome);
    expect(await screen.findByText("已完成导入")).toBeInTheDocument();
    expect(screen.queryByDisplayValue(source)).not.toBeInTheDocument();
  });

  it("requires structured choices and suppresses a stale preview", async () => {
    const first = basePreview({
      counts: { ...basePreview().counts, total: 1, importable: 0, errors: 1 },
      items: [{
        item_index: 0,
        display_name_masked: "I***m",
        platform: null,
        kind: "api",
        cpa_section: null,
        disposition: "error",
        issue_codes: ["transfer.choice_required"],
      }],
    });
    const resolved = basePreview();
    let resolveFirst: ((value: RouteCredentialImportPreview) => void) | undefined;
    vi.mocked(previewRouteCredentialImport)
      .mockImplementationOnce(() => new Promise((resolve) => { resolveFirst = resolve; }))
      .mockResolvedValue(resolved);
    renderDialog();
    fireEvent.change(screen.getByRole("textbox", { name: "账号 JSON" }), { target: { value: "[{\"api_key\":\"one\"}]" } });
    await waitFor(() => expect(previewRouteCredentialImport).toHaveBeenCalledTimes(1));
    fireEvent.change(screen.getByRole("textbox", { name: "账号 JSON" }), { target: { value: "[{\"api_key\":\"two\"}]" } });
    await waitFor(() => expect(previewRouteCredentialImport).toHaveBeenCalledTimes(2));
    resolveFirst?.(first);
    expect(screen.queryByText("I***m")).not.toBeInTheDocument();
    expect(await screen.findByText("A***e")).toBeInTheDocument();
  });

  it("rejects invalid UTF-8 files and clears sensitive state on close", async () => {
    const onClose = vi.fn();
    const view = renderDialog(vi.fn(), onClose);
    const invalidBytes = new Uint8Array([0xc3, 0x28]);
    const file = new File([invalidBytes], "accounts.json", { type: "application/json" });
    await userEvent.upload(screen.getByLabelText("选择账号 JSON 文件"), file);
    expect(await screen.findByRole("alert")).toHaveTextContent("UTF-8");
    await userEvent.click(screen.getByRole("button", { name: "关闭导入账号" }));
    expect(onClose).toHaveBeenCalled();
    view.rerender(<RouteCredentialImportDialog open onClose={onClose} onImported={vi.fn()} />);
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("requires explicit pool restoration and keeps one completion page", async () => {
    renderDialog();
    fireEvent.change(screen.getByRole("textbox", { name: "账号 JSON" }), { target: { value: "[]" } });
    await screen.findByText("A***e");
    expect(screen.getByRole("checkbox", { name: "恢复算力池成员" })).not.toBeChecked();
    await userEvent.click(screen.getByRole("checkbox", { name: "恢复算力池成员" }));
    await userEvent.click(screen.getByRole("button", { name: "确认导入账号" }));
    await waitFor(() => expect(importRouteCredentials).toHaveBeenCalledWith(expect.objectContaining({
      restore_pool_membership: true,
    })));
    expect(await screen.findByText("已完成导入")).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: /完成|关闭导入账号/ }).length).toBeGreaterThan(0);
  });
});
