import { StrictMode } from "react";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { RouteCredentialExportDialog } from "../src/components/accounts/RouteCredentialExportDialog";
import {
  exportRouteCredentials,
  saveRouteCredentialExport,
} from "../src/lib/api/client";
import type { RouteCredentialExportResult } from "../src/lib/api/types";
import {
  copySensitiveText,
  downloadRouteCredentialJson,
} from "../src/lib/routeCredentialTransfer";
import { isDesktop } from "../src/lib/transport";
import { I18nProvider } from "../src/lib/i18n";

vi.mock("../src/lib/api/client", () => ({
  exportRouteCredentials: vi.fn(),
  saveRouteCredentialExport: vi.fn(),
}));

vi.mock("../src/lib/routeCredentialTransfer", () => ({
  copySensitiveText: vi.fn(),
  downloadRouteCredentialJson: vi.fn(),
}));

vi.mock("../src/lib/transport", () => ({
  isDesktop: vi.fn(),
}));

const selectionContext = { platform: "claude", pool_scope: "in_pool" } as const;
const credentialIds = ["credential-3", "credential-1", "credential-2"];
const jsonText = '[{"api_key":"secret"}]\n';

function exportResult(
  overrides: Partial<RouteCredentialExportResult> = {},
): RouteCredentialExportResult {
  return {
    json_text: jsonText,
    suggested_file_name: "ai-switch-claude-route-credentials.json",
    counts: { total: 3, official: 1, api: 2 },
    scheme_links: [
      {
        credential_id: "credential-1",
        display_name: "Production API",
        url: "ccswitch://import?api_key=secret",
        issue_code: null,
      },
      {
        credential_id: "credential-2",
        display_name: "Legacy API",
        url: null,
        issue_code: "transfer.scheme_unsupported",
      },
    ],
    warnings: [
      {
        display_name: "Legacy API",
        code: "transfer.scheme_unsupported",
      },
    ],
    errors: [],
    ...overrides,
  };
}

function renderDialog(open = true, language: "en" | "zh-CN" = "en") {
  return render(
    <RouteCredentialExportDialog
      open={open}
      selection_context={selectionContext}
      credential_ids={credentialIds}
      onClose={vi.fn()}
    />,
    {
      wrapper: ({ children }) => <I18nProvider initialLanguage={language}>{children}</I18nProvider>,
    },
  );
}

describe("RouteCredentialExportDialog", () => {
  beforeEach(() => {
    window.localStorage.removeItem("ai-switch.language");
    vi.mocked(exportRouteCredentials).mockReset();
    vi.mocked(saveRouteCredentialExport).mockReset();
    vi.mocked(copySensitiveText).mockReset();
    vi.mocked(downloadRouteCredentialJson).mockReset();
    vi.mocked(isDesktop).mockReset();
    vi.mocked(isDesktop).mockReturnValue(false);
    vi.mocked(exportRouteCredentials).mockResolvedValue(exportResult());
    vi.mocked(saveRouteCredentialExport).mockResolvedValue({ cancelled: false });
    vi.mocked(copySensitiveText).mockResolvedValue();
  });

  it("exports every supplied ID and regenerates once when metadata changes", async () => {
    renderDialog();

    await waitFor(() => {
      expect(exportRouteCredentials).toHaveBeenCalledTimes(1);
    });
    expect(exportRouteCredentials).toHaveBeenLastCalledWith({
      selection_context: selectionContext,
      credential_ids: credentialIds,
      include_enhanced_metadata: true,
    });

    await userEvent.click(screen.getByRole("checkbox", { name: "Include enhanced metadata" }));

    await waitFor(() => {
      expect(exportRouteCredentials).toHaveBeenCalledTimes(2);
    });
    expect(exportRouteCredentials).toHaveBeenLastCalledWith({
      selection_context: selectionContext,
      credential_ids: credentialIds,
      include_enhanced_metadata: false,
    });
  });

  it("generates only once when mounted under React Strict Mode", async () => {
    render(
      <StrictMode>
        <RouteCredentialExportDialog
          open
          selection_context={selectionContext}
          credential_ids={credentialIds}
          onClose={vi.fn()}
        />
      </StrictMode>,
    );

    await waitFor(() => {
      expect(exportRouteCredentials).toHaveBeenCalledTimes(1);
    });
    await new Promise((resolve) => setTimeout(resolve, 25));
    expect(exportRouteCredentials).toHaveBeenCalledTimes(1);
  });

  it("shows migration JSON, scheme links, warnings, and clipboard risk", async () => {
    renderDialog();

    expect(await screen.findByText(jsonText.trim())).toBeInTheDocument();
    expect(screen.getByText(/contains credentials/i)).toBeInTheDocument();
    expect(screen.getByText("Legacy API")).toBeInTheDocument();
    expect(screen.getByText("transfer.scheme_unsupported")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("tab", { name: "Scheme links" }));

    expect(screen.getByText("ccswitch://import?api_key=secret")).toBeInTheDocument();
    expect(screen.getByText(/API keys.*system clipboard/i)).toBeInTheDocument();
    expect(screen.getByText("Production API")).toBeInTheDocument();
  });

  it("renders export controls in Chinese", async () => {
    renderDialog(true, "zh-CN");

    expect(await screen.findByRole("heading", { name: "导出路由凭据" })).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: "包含增强元数据" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "复制迁移 JSON" })).toBeInTheDocument();

    await userEvent.click(screen.getByRole("tab", { name: "方案链接" }));
    expect(screen.getByText(/复制方案链接会将 API 密钥放入系统剪贴板/)).toBeInTheDocument();
  });

  it("confirms before copying a scheme URL containing an API key", async () => {
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    renderDialog();

    await screen.findByText(jsonText.trim());
    await userEvent.click(screen.getByRole("tab", { name: "Scheme links" }));
    await userEvent.click(
      screen.getByRole("button", { name: "Copy scheme URL for Production API" }),
    );

    expect(confirm).toHaveBeenCalledWith(
      "This scheme URL contains an API key. Copy it to the system clipboard?",
    );
    expect(copySensitiveText).toHaveBeenCalledWith("ccswitch://import?api_key=secret");
  });

  it("disables sensitive actions when export has blocking errors", async () => {
    vi.mocked(exportRouteCredentials).mockResolvedValue(
      exportResult({
        errors: [{ display_name: "Missing account", code: "transfer.selection_invalid" }],
      }),
    );

    renderDialog();

    expect(await screen.findByText("transfer.selection_invalid")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Copy migration JSON" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Download JSON" })).toBeDisabled();

    await userEvent.click(screen.getByRole("tab", { name: "Scheme links" }));
    expect(
      screen.getByRole("button", { name: "Copy scheme URL for Production API" }),
    ).toBeDisabled();
  });

  it("uses the exact non-null JSON contract even when the payload is empty", async () => {
    vi.mocked(exportRouteCredentials).mockResolvedValue(exportResult({ json_text: "" }));
    renderDialog();

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Copy migration JSON" })).toBeEnabled();
    });
    await userEvent.click(screen.getByRole("button", { name: "Copy migration JSON" }));
    await userEvent.click(screen.getByRole("button", { name: "Download JSON" }));

    expect(copySensitiveText).toHaveBeenCalledWith("");
    expect(downloadRouteCredentialJson).toHaveBeenCalledWith(
      "",
      "ai-switch-claude-route-credentials.json",
    );
  });

  it("copies and saves the identical returned JSON on Desktop", async () => {
    vi.mocked(isDesktop).mockReturnValue(true);
    renderDialog();

    await screen.findByText(jsonText.trim());
    await userEvent.click(screen.getByRole("button", { name: "Copy migration JSON" }));
    await userEvent.click(screen.getByRole("button", { name: "Save JSON" }));

    expect(copySensitiveText).toHaveBeenCalledWith(jsonText);
    expect(saveRouteCredentialExport).toHaveBeenCalledWith({
      suggested_file_name: "ai-switch-claude-route-credentials.json",
      json_text: jsonText,
    });
    expect(exportRouteCredentials).toHaveBeenCalledTimes(1);
    expect(downloadRouteCredentialJson).not.toHaveBeenCalled();
  });

  it("downloads the identical returned JSON on Web without regenerating", async () => {
    renderDialog();

    await screen.findByText(jsonText.trim());
    await userEvent.click(screen.getByRole("button", { name: "Download JSON" }));

    expect(downloadRouteCredentialJson).toHaveBeenCalledWith(
      jsonText,
      "ai-switch-claude-route-credentials.json",
    );
    expect(exportRouteCredentials).toHaveBeenCalledTimes(1);
    expect(saveRouteCredentialExport).not.toHaveBeenCalled();
  });

  it("keeps JSON available after a clipboard rejection", async () => {
    vi.mocked(copySensitiveText).mockRejectedValue(new Error("clipboard denied"));
    renderDialog();

    await screen.findByText(jsonText.trim());
    await userEvent.click(screen.getByRole("button", { name: "Copy migration JSON" }));

    expect(await screen.findByText("clipboard denied")).toBeInTheDocument();
    expect(screen.getByText(jsonText.trim())).toBeInTheDocument();
  });

  it("clears sensitive JSON and links when closed and while reopening", async () => {
    const nextExport = new Promise<RouteCredentialExportResult>(() => {});
    const view = renderDialog();
    await screen.findByText(jsonText.trim());
    await userEvent.click(screen.getByRole("tab", { name: "Scheme links" }));
    expect(screen.getByText("ccswitch://import?api_key=secret")).toBeInTheDocument();

    view.rerender(
      <RouteCredentialExportDialog
        open={false}
        selection_context={selectionContext}
        credential_ids={credentialIds}
        onClose={vi.fn()}
      />,
    );
    expect(screen.queryByText(jsonText.trim())).not.toBeInTheDocument();
    expect(screen.queryByText("ccswitch://import?api_key=secret")).not.toBeInTheDocument();

    vi.mocked(exportRouteCredentials).mockReturnValue(nextExport);
    view.rerender(
      <RouteCredentialExportDialog
        open
        selection_context={selectionContext}
        credential_ids={credentialIds}
        onClose={vi.fn()}
      />,
    );

    expect(await screen.findByText("Generating export..." )).toBeInTheDocument();
    expect(screen.queryByText(jsonText.trim())).not.toBeInTheDocument();
    view.unmount();
  });

  it("moves focus into the modal and restores it when the dialog closes", async () => {
    const launcher = document.createElement("button");
    document.body.appendChild(launcher);
    launcher.focus();
    const view = renderDialog();

    expect(screen.getByRole("button", { name: "Close export dialog" })).toHaveFocus();

    view.rerender(
      <RouteCredentialExportDialog
        open={false}
        selection_context={selectionContext}
        credential_ids={credentialIds}
        onClose={vi.fn()}
      />,
    );
    expect(launcher).toHaveFocus();
    launcher.remove();
  });

  it("traps focus in the dialog and supports arrow-key tab navigation", async () => {
    const user = userEvent.setup();
    renderDialog();
    await screen.findByText(jsonText.trim());

    const close = screen.getByRole("button", { name: "Close export dialog" });
    const download = screen.getByRole("button", { name: "Download JSON" });
    close.focus();
    await user.keyboard("{Shift>}{Tab}{/Shift}");
    expect(download).toHaveFocus();
    await user.tab();
    expect(close).toHaveFocus();

    const jsonTab = screen.getByRole("tab", { name: "Migration JSON" });
    jsonTab.focus();
    await user.keyboard("{ArrowRight}");
    expect(screen.getByRole("tab", { name: "Scheme links" })).toHaveFocus();
    expect(screen.getByRole("tab", { name: "Scheme links" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
  });

  it("keeps the opened selection summary and export snapshot when props change", async () => {
    const view = renderDialog();
    await screen.findByText(jsonText.trim());

    view.rerender(
      <RouteCredentialExportDialog
        open
        selection_context={{ platform: "codex", pool_scope: "out_of_pool" }}
        credential_ids={["replacement"]}
        onClose={vi.fn()}
      />,
    );

    expect(screen.getByText("3 selected · claude · in_pool")).toBeInTheDocument();
    expect(exportRouteCredentials).toHaveBeenCalledTimes(1);
    expect(exportRouteCredentials).toHaveBeenCalledWith({
      selection_context: selectionContext,
      credential_ids: credentialIds,
      include_enhanced_metadata: true,
    });
  });
});
