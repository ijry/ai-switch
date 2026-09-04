import {
  AlertTriangle,
  CheckCircle2,
  Clipboard,
  Download,
  Save,
  X,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import {
  exportRouteCredentials,
  saveRouteCredentialExport,
} from "../../lib/api/client";
import { useI18n } from "../../lib/i18n";
import type {
  RouteCredentialExportResult,
  RouteCredentialSelectionContext,
  RouteCredentialTransferIssue,
} from "../../lib/api/types";
import {
  copySensitiveText,
  downloadRouteCredentialJson,
} from "../../lib/routeCredentialTransfer";
import { isDesktop } from "../../lib/transport";
import { Button } from "../ui/Button";

export type RouteCredentialExportDialogProps = {
  open: boolean;
  selection_context: RouteCredentialSelectionContext;
  credential_ids: string[];
  onClose: () => void;
};

type ExportTab = "json" | "links";

type SelectionSnapshot = {
  selection_context: RouteCredentialSelectionContext;
  credential_ids: string[];
};

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function issueLabel(issue: RouteCredentialTransferIssue, t: ReturnType<typeof useI18n>["t"]): string {
  return issue.display_name?.trim() || t("routeExport.selectedItem", { number: (issue.item_index ?? 0) + 1 });
}

export function RouteCredentialExportDialog({
  open,
  selection_context,
  credential_ids,
  onClose,
}: RouteCredentialExportDialogProps) {
  const { t } = useI18n();
  const [selectionSnapshot, setSelectionSnapshot] = useState<SelectionSnapshot | null>(null);
  const [includeEnhancedMetadata, setIncludeEnhancedMetadata] = useState(true);
  const [activeTab, setActiveTab] = useState<ExportTab>("json");
  const [result, setResult] = useState<RouteCredentialExportResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [requestError, setRequestError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [actionStatus, setActionStatus] = useState<string | null>(null);
  const [actionPending, setActionPending] = useState(false);
  const dialogRef = useRef<HTMLDivElement>(null);
  const closeButtonRef = useRef<HTMLButtonElement>(null);
  const jsonTabRef = useRef<HTMLButtonElement>(null);
  const linksTabRef = useRef<HTMLButtonElement>(null);
  const requestSequenceRef = useRef(0);
  const desktop = isDesktop();

  useEffect(() => {
    if (open) {
      setSelectionSnapshot((current) =>
        current ?? {
          selection_context: { ...selection_context },
          credential_ids: [...credential_ids],
        },
      );
      return;
    }

    requestSequenceRef.current += 1;
    setSelectionSnapshot(null);
    setIncludeEnhancedMetadata(true);
    setActiveTab("json");
    setResult(null);
    setLoading(false);
    setRequestError(null);
    setActionError(null);
    setActionStatus(null);
    setActionPending(false);
  }, [credential_ids, open, selection_context]);

  useEffect(() => {
    if (!open || !selectionSnapshot) {
      return;
    }

    const requestSequence = ++requestSequenceRef.current;
    setLoading(true);
    setResult(null);
    setRequestError(null);
    setActionError(null);
    setActionStatus(null);

    void exportRouteCredentials({
      selection_context: selectionSnapshot.selection_context,
      credential_ids: selectionSnapshot.credential_ids,
      include_enhanced_metadata: includeEnhancedMetadata,
    }).then(
      (nextResult) => {
        if (requestSequence !== requestSequenceRef.current) {
          return;
        }
        setResult(nextResult);
        setLoading(false);
      },
      (error: unknown) => {
        if (requestSequence !== requestSequenceRef.current) {
          return;
        }
        setRequestError(errorMessage(error));
        setLoading(false);
      },
    );

    return () => {
      if (requestSequence === requestSequenceRef.current) {
        requestSequenceRef.current += 1;
      }
    };
  }, [includeEnhancedMetadata, open, selectionSnapshot]);

  useEffect(() => {
    if (!open) {
      return;
    }

    const previousFocus = document.activeElement instanceof HTMLElement
      ? document.activeElement
      : null;
    closeButtonRef.current?.focus();

    return () => {
      if (previousFocus?.isConnected) {
        previousFocus.focus();
      }
    };
  }, [open]);

  useEffect(() => {
    if (!open) {
      return;
    }

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        handleClose();
        return;
      }

      if (event.key !== "Tab") {
        return;
      }

      const dialog = dialogRef.current;
      if (!dialog) {
        return;
      }

      const focusable = Array.from(
        dialog.querySelectorAll<HTMLElement>(
          'button:not([disabled]), input:not([disabled]), [href], [tabindex]:not([tabindex="-1"])',
        ),
      ).filter((element) => !element.hasAttribute("hidden"));
      const first = focusable[0];
      const last = focusable.at(-1);
      if (!first || !last) {
        event.preventDefault();
        dialog.focus();
        return;
      }

      if (!dialog.contains(document.activeElement)) {
        event.preventDefault();
        first.focus();
      } else if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [open]);

  if (!open) {
    return null;
  }

  const hasBlockingErrors = Boolean(result?.errors.length) || result?.json_text == null;
  const sensitiveActionsDisabled = loading || actionPending || hasBlockingErrors;
  const displayedSelectionContext = selectionSnapshot?.selection_context ?? selection_context;
  const displayedCredentialCount = selectionSnapshot?.credential_ids.length ?? credential_ids.length;

  function clearSensitiveState() {
    requestSequenceRef.current += 1;
    setResult(null);
    setSelectionSnapshot(null);
    setRequestError(null);
    setActionError(null);
    setActionStatus(null);
  }

  function handleClose() {
    clearSensitiveState();
    onClose();
  }

  async function handleCopyJson() {
    if (result?.json_text == null || hasBlockingErrors) {
      return;
    }

    setActionPending(true);
    setActionError(null);
    setActionStatus(null);
    try {
      await copySensitiveText(result.json_text);
      setActionStatus(t("routeExport.copyJsonStatus"));
    } catch (error) {
      setActionError(errorMessage(error));
    } finally {
      setActionPending(false);
    }
  }

  async function handleSaveJson() {
    if (result?.json_text == null || hasBlockingErrors) {
      return;
    }

    setActionPending(true);
    setActionError(null);
    setActionStatus(null);
    try {
      if (desktop) {
        const saveResult = await saveRouteCredentialExport({
          suggested_file_name: result.suggested_file_name,
          json_text: result.json_text,
        });
        setActionStatus(saveResult.cancelled ? t("routeExport.saveCancelled") : t("routeExport.exportSaved"));
      } else {
        downloadRouteCredentialJson(result.json_text, result.suggested_file_name);
        setActionStatus(t("routeExport.downloadStarted"));
      }
    } catch (error) {
      setActionError(errorMessage(error));
    } finally {
      setActionPending(false);
    }
  }

  async function handleCopySchemeLink(url: string) {
    const confirmed = window.confirm(
      t("routeExport.schemeConfirm"),
    );
    if (!confirmed) {
      return;
    }

    setActionPending(true);
    setActionError(null);
    setActionStatus(null);
    try {
      await copySensitiveText(url);
      setActionStatus(t("routeExport.schemeCopied"));
    } catch (error) {
      setActionError(errorMessage(error));
    } finally {
      setActionPending(false);
    }
  }

  function handleTabKeyDown(event: React.KeyboardEvent<HTMLButtonElement>) {
    const tabs = [jsonTabRef.current, linksTabRef.current];
    const currentIndex = tabs.indexOf(event.currentTarget);
    let nextIndex: number | null = null;

    if (event.key === "ArrowRight") {
      nextIndex = (currentIndex + 1) % tabs.length;
    } else if (event.key === "ArrowLeft") {
      nextIndex = (currentIndex - 1 + tabs.length) % tabs.length;
    } else if (event.key === "Home") {
      nextIndex = 0;
    } else if (event.key === "End") {
      nextIndex = tabs.length - 1;
    }

    if (nextIndex == null) {
      return;
    }

    event.preventDefault();
    setActiveTab(nextIndex === 0 ? "json" : "links");
    tabs[nextIndex]?.focus();
  }

  return (
    <div
      className="motion-overlay fixed inset-0 z-[80] flex items-center justify-center bg-stone-950/45 p-4"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) {
          handleClose();
        }
      }}
    >
      <div
        ref={dialogRef}
        aria-labelledby="route-credential-export-title"
        aria-modal="true"
        className="flex max-h-[min(760px,calc(100vh-2rem))] w-full max-w-2xl flex-col overflow-hidden rounded-lg border border-stone-200 bg-white shadow-2xl"
        role="dialog"
        tabIndex={-1}
      >
        <header className="flex items-start justify-between gap-4 border-b border-stone-200 px-5 py-4">
          <div className="min-w-0">
            <h2 id="route-credential-export-title" className="text-base font-semibold text-stone-950">
              {t("routeExport.title")}
            </h2>
            <p className="mt-1 text-xs text-stone-500">
              {t("routeExport.selectedSummary", {
                count: displayedCredentialCount,
                platform: displayedSelectionContext.platform,
                scope: displayedSelectionContext.pool_scope,
              })}
            </p>
          </div>
          <button
            ref={closeButtonRef}
            aria-label={t("routeExport.closeAria")}
            className="grid h-8 w-8 shrink-0 place-items-center rounded-md text-stone-500 motion-control hover:bg-stone-100 hover:text-stone-900 focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-500"
            onClick={handleClose}
            title={t("routeExport.close")}
            type="button"
          >
            <X aria-hidden="true" className="h-4 w-4" />
          </button>
        </header>

        <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto px-5 py-4">
          <div className="flex items-start gap-2 rounded-md border border-amber-200 bg-amber-50 px-3 py-2.5 text-xs leading-5 text-amber-950">
            <AlertTriangle aria-hidden="true" className="mt-0.5 h-4 w-4 shrink-0 text-amber-700" />
            <p>
              {t("routeExport.securityNotice")}
            </p>
          </div>

          <label className="flex cursor-pointer items-center justify-between gap-4 rounded-md border border-stone-200 px-3 py-2.5">
            <span>
              <span className="block text-sm font-medium text-stone-900">{t("routeExport.includeMetadata")}</span>
              <span className="block text-xs text-stone-500">{t("routeExport.metadataHelp")}</span>
            </span>
            <input
              aria-label={t("routeExport.includeMetadata")}
              checked={includeEnhancedMetadata}
              className="h-4 w-4 accent-stone-900"
              disabled={loading}
              onChange={(event) => setIncludeEnhancedMetadata(event.target.checked)}
              type="checkbox"
            />
          </label>

          {loading || !selectionSnapshot ? (
            <div aria-live="polite" className="rounded-md border border-stone-200 bg-stone-50 px-3 py-4 text-sm text-stone-600">
              {t("routeExport.generating")}
            </div>
          ) : null}

          {requestError ? (
            <div role="alert" className="rounded-md border border-red-200 bg-red-50 px-3 py-2.5 text-sm text-red-800">
              {requestError}
            </div>
          ) : null}

          {result ? (
            <>
              <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-stone-600">
                <span>{t("routeExport.total", { count: result.counts.total })}</span>
                <span>{t("routeExport.official", { count: result.counts.official })}</span>
                <span>{t("routeExport.api", { count: result.counts.api })}</span>
                {!hasBlockingErrors ? (
                  <span className="inline-flex items-center gap-1 font-medium text-emerald-700">
                    <CheckCircle2 aria-hidden="true" className="h-3.5 w-3.5" />
                    {t("routeExport.generated")}
                  </span>
                ) : null}
              </div>

              {result.errors.length ? (
                <section aria-label={t("routeExport.errorsAria")} className="rounded-md border border-red-200 bg-red-50 px-3 py-2.5">
                  <h3 className="text-xs font-semibold uppercase text-red-800">{t("routeExport.blockingErrors")}</h3>
                  <ul className="mt-1.5 space-y-1 text-xs text-red-800">
                    {result.errors.map((issue, index) => (
                      <li key={`${issue.code}-${issue.item_index ?? index}`}>
                        <span className="font-medium">{issueLabel(issue, t)}</span>: {" "}
                        <code>{issue.code}</code>
                      </li>
                    ))}
                  </ul>
                </section>
              ) : null}

              {result.warnings.length ? (
                <section aria-label={t("routeExport.warningsAria")} className="rounded-md border border-amber-200 bg-amber-50 px-3 py-2.5">
                  <h3 className="text-xs font-semibold uppercase text-amber-900">{t("routeExport.warnings")}</h3>
                  <ul className="mt-1.5 space-y-1 text-xs text-amber-900">
                    {result.warnings.map((issue, index) => (
                      <li key={`${issue.code}-${issue.item_index ?? index}`}>
                        <span className="font-medium">{issueLabel(issue, t)}</span>: {" "}
                        <code>{issue.code}</code>
                      </li>
                    ))}
                  </ul>
                </section>
              ) : null}

              <div className="flex min-h-0 flex-1 flex-col">
                <div aria-label={t("routeExport.formatsAria")} className="flex w-fit rounded-md bg-stone-100 p-1" role="tablist">
                  <button
                    ref={jsonTabRef}
                    aria-controls="route-export-json-panel"
                    aria-selected={activeTab === "json"}
                    className={`rounded px-3 py-1.5 text-xs font-medium motion-control focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-500 ${
                      activeTab === "json" ? "bg-white text-stone-950 shadow-sm" : "text-stone-600 hover:text-stone-950"
                    }`}
                    id="route-export-json-tab"
                    onClick={() => setActiveTab("json")}
                    onKeyDown={handleTabKeyDown}
                    role="tab"
                    tabIndex={activeTab === "json" ? 0 : -1}
                    type="button"
                  >
                    {t("routeExport.jsonTab")}
                  </button>
                  <button
                    ref={linksTabRef}
                    aria-controls="route-export-links-panel"
                    aria-selected={activeTab === "links"}
                    className={`rounded px-3 py-1.5 text-xs font-medium motion-control focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-500 ${
                      activeTab === "links" ? "bg-white text-stone-950 shadow-sm" : "text-stone-600 hover:text-stone-950"
                    }`}
                    id="route-export-links-tab"
                    onClick={() => setActiveTab("links")}
                    onKeyDown={handleTabKeyDown}
                    role="tab"
                    tabIndex={activeTab === "links" ? 0 : -1}
                    type="button"
                  >
                    {t("routeExport.linksTab")}
                  </button>
                </div>

                {activeTab === "json" ? (
                  <div
                    aria-labelledby="route-export-json-tab"
                    className="mt-3 min-h-44 flex-1 overflow-auto rounded-md border border-stone-200 bg-stone-950 p-3"
                    id="route-export-json-panel"
                    role="tabpanel"
                  >
                    <pre className="whitespace-pre-wrap break-all font-mono text-xs leading-5 text-stone-100">
                      {result.json_text ?? t("routeExport.jsonUnavailable")}
                    </pre>
                  </div>
                ) : (
                  <div
                    aria-labelledby="route-export-links-tab"
                    className="mt-3 space-y-3"
                    id="route-export-links-panel"
                    role="tabpanel"
                  >
                    <div className="flex items-start gap-2 rounded-md border border-red-200 bg-red-50 px-3 py-2.5 text-xs leading-5 text-red-800">
                      <AlertTriangle aria-hidden="true" className="mt-0.5 h-4 w-4 shrink-0" />
                      <p>{t("routeExport.linksSecurityNotice")}</p>
                    </div>
                    {result.scheme_links.length ? (
                      <ul className="space-y-2">
                        {result.scheme_links.map((link) => (
                          <li key={link.credential_id} className="rounded-md border border-stone-200 px-3 py-2.5">
                            <div className="flex items-start justify-between gap-3">
                              <div className="min-w-0">
                                <p className="text-sm font-medium text-stone-900">{link.display_name}</p>
                                {link.url ? (
                                  <p className="mt-1 break-all font-mono text-xs leading-5 text-stone-600">{link.url}</p>
                                ) : (
                                  <p className="mt-1 text-xs text-amber-800">{link.issue_code ?? t("routeExport.schemeUnavailable")}</p>
                                )}
                              </div>
                              <button
                                aria-label={t("routeExport.copySchemeAria", { name: link.display_name })}
                                className="grid h-8 w-8 shrink-0 place-items-center rounded-md border border-stone-200 text-stone-600 motion-control hover:bg-stone-100 hover:text-stone-950 focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-500 disabled:cursor-not-allowed disabled:opacity-40"
                                disabled={!link.url || sensitiveActionsDisabled}
                                onClick={() => link.url && void handleCopySchemeLink(link.url)}
                                title={t("routeExport.copySchemeTitle")}
                                type="button"
                              >
                                <Clipboard aria-hidden="true" className="h-4 w-4" />
                              </button>
                            </div>
                          </li>
                        ))}
                      </ul>
                    ) : (
                      <p className="rounded-md border border-stone-200 bg-stone-50 px-3 py-4 text-sm text-stone-600">
                        {t("routeExport.noSchemeLinks")}
                      </p>
                    )}
                  </div>
                )}
              </div>
            </>
          ) : null}

          {actionError ? <p role="alert" className="text-sm text-red-700">{actionError}</p> : null}
          {actionStatus ? <p aria-live="polite" className="text-sm text-emerald-700">{actionStatus}</p> : null}
        </div>

        <footer className="flex flex-wrap items-center justify-end gap-2 border-t border-stone-200 px-5 py-3">
          <Button type="button" variant="secondary" onClick={handleClose}>
            {t("routeExport.close")}
          </Button>
          <Button
            aria-label={t("routeExport.copyJsonAria")}
            className="inline-flex items-center gap-1.5"
            disabled={sensitiveActionsDisabled}
            onClick={() => void handleCopyJson()}
            type="button"
            variant="secondary"
          >
            <Clipboard aria-hidden="true" className="h-4 w-4" />
            {t("routeExport.copyJson")}
          </Button>
          <Button
            aria-label={desktop ? t("routeExport.saveJson") : t("routeExport.downloadJson")}
            className="inline-flex items-center gap-1.5"
            disabled={sensitiveActionsDisabled}
            onClick={() => void handleSaveJson()}
            type="button"
          >
            {desktop ? <Save aria-hidden="true" className="h-4 w-4" /> : <Download aria-hidden="true" className="h-4 w-4" />}
            {desktop ? t("routeExport.saveJson") : t("routeExport.downloadJson")}
          </Button>
        </footer>
      </div>
    </div>
  );
}
