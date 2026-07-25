import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  AlertTriangle,
  FolderOpen,
  KeyRound,
  LockKeyhole,
  RefreshCcw,
  RotateCcw,
  ShieldCheck,
  Trash2,
  Unplug,
} from "lucide-react";
import {
  deleteRouteProxyHttpsCertificates,
  disableRouteProxyHttps,
  enableRouteProxyHttps,
  getRouteProxyHttpsStatus,
  getRouteProxyStatus,
  openRouteProxyHttpsCertificateDirectory,
  regenerateRouteProxyHttpsCertificates,
  reimportRouteProxyRootCa,
  uninstallRouteProxyRootCa,
} from "../../lib/api/client";
import type {
  RouteProxyHttpsOperationOutcome,
  RouteProxyHttpsStatus,
  RouteProxyTrustStatus,
} from "../../lib/api/types";
import { useI18n } from "../../lib/i18n";
import { isDesktop } from "../../lib/transport";

const queryKeys = {
  https: ["route-proxy-https-status"],
  proxy: ["route-proxy-status"],
} as const;

function trustStatusLabel(status: RouteProxyTrustStatus, t: ReturnType<typeof useI18n>["t"]) {
  switch (status) {
    case "systemTrusted":
    case "nssTrusted":
      return t("settings.https.trustTrusted");
    case "partiallyTrusted":
      return t("settings.https.trustPartial");
    case "untrusted":
      return t("settings.https.trustUntrusted");
    default:
      return t("settings.https.trustUnknown");
  }
}

function syncOutcome(
  queryClient: ReturnType<typeof useQueryClient>,
  outcome: RouteProxyHttpsOperationOutcome,
) {
  queryClient.setQueryData(queryKeys.https, outcome.https);
  queryClient.setQueryData(queryKeys.proxy, outcome.routeProxy);
}

function actionError(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

export function RouteProxyHttpsSettings() {
  const queryClient = useQueryClient();
  const { t } = useI18n();
  const httpsQuery = useQuery({
    queryKey: queryKeys.https,
    queryFn: getRouteProxyHttpsStatus,
  });
  const proxyQuery = useQuery({
    queryKey: queryKeys.proxy,
    queryFn: getRouteProxyStatus,
  });

  const enableMutation = useMutation({
    mutationFn: enableRouteProxyHttps,
    onSuccess: (outcome) => syncOutcome(queryClient, outcome),
  });
  const disableMutation = useMutation({
    mutationFn: disableRouteProxyHttps,
    onSuccess: (outcome) => syncOutcome(queryClient, outcome),
  });
  const reimportMutation = useMutation({
    mutationFn: reimportRouteProxyRootCa,
    onSuccess: (outcome) => syncOutcome(queryClient, outcome),
  });
  const regenerateMutation = useMutation({
    mutationFn: regenerateRouteProxyHttpsCertificates,
    onSuccess: (outcome) => syncOutcome(queryClient, outcome),
  });
  const uninstallMutation = useMutation({
    mutationFn: uninstallRouteProxyRootCa,
    onSuccess: (outcome) => syncOutcome(queryClient, outcome),
  });
  const deleteMutation = useMutation({
    mutationFn: deleteRouteProxyHttpsCertificates,
    onSuccess: async (https) => {
      queryClient.setQueryData(queryKeys.https, https);
      await queryClient.invalidateQueries({ queryKey: queryKeys.proxy });
    },
  });
  const openDirectoryMutation = useMutation({
    mutationFn: openRouteProxyHttpsCertificateDirectory,
  });

  const https = httpsQuery.data;
  const isMutating =
    enableMutation.isPending ||
    disableMutation.isPending ||
    reimportMutation.isPending ||
    regenerateMutation.isPending ||
    uninstallMutation.isPending ||
    deleteMutation.isPending;
  const mutationError = [
    enableMutation.error,
    disableMutation.error,
    reimportMutation.error,
    regenerateMutation.error,
    uninstallMutation.error,
    deleteMutation.error,
    openDirectoryMutation.error,
  ].find(Boolean);

  const confirmAndRun = (message: string, action: () => void) => {
    if (window.confirm(message)) {
      action();
    }
  };

  const statusLabel = !https?.enabled
    ? t("settings.https.statusDisabled")
    : https.certReady
      ? t("settings.https.statusCertificateReady")
      : t("settings.https.statusUnknown");

  return (
    <section className="space-y-3 rounded-2xl border border-stone-200 bg-white/82 p-4 shadow-sm">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <span className="grid h-8 w-8 place-items-center rounded-xl bg-stone-950 text-white">
              <LockKeyhole className="h-4 w-4" />
            </span>
            <div>
              <h2 className="text-[15px] font-semibold text-stone-950">{t("settings.https.title")}</h2>
              <p className="text-[12px] text-stone-500">
                <span className="font-medium text-stone-600">{t("settings.https.localPool")}</span>{" "}
                {t("settings.https.subtitle")}
              </p>
            </div>
          </div>
        </div>
        <button
          className="inline-flex items-center gap-1.5 rounded-xl border border-stone-200 bg-stone-50 px-3 py-2 text-[12px] font-semibold text-stone-700 transition-colors hover:border-stone-300 hover:bg-white"
          onClick={() => {
            void httpsQuery.refetch();
            void proxyQuery.refetch();
          }}
          type="button"
        >
          <RefreshCcw className="h-3.5 w-3.5" />
          {t("settings.https.refresh")}
        </button>
      </div>

      {httpsQuery.isLoading ? (
        <p className="text-[12px] text-stone-500">{t("settings.https.loading")}</p>
      ) : !https ? (
        <p className="rounded-xl border border-red-200 bg-red-50 px-3 py-2 text-[12px] text-red-700">
          {t("settings.https.loadError")}
        </p>
      ) : (
        <>
          <label className="inline-flex items-center gap-2 rounded-xl border border-stone-200 bg-stone-50 px-3 py-2 text-[12px] font-medium text-stone-700">
            <input
              aria-label={t("settings.https.enabled")}
              checked={https.enabled}
              disabled={isMutating}
              onChange={(event) => {
                if (event.target.checked) {
                  enableMutation.mutate();
                  return;
                }
                disableMutation.mutate();
              }}
              type="checkbox"
            />
            {t("settings.https.enabled")}
          </label>

          <div className="grid gap-2 rounded-xl border border-stone-200 bg-stone-50 p-3 text-[12px] text-stone-600 sm:grid-cols-2">
            <p>
              <span className="font-semibold text-stone-800">{t("settings.https.status")}:</span> {statusLabel}
            </p>
            <p>
              <span className="font-semibold text-stone-800">{t("settings.https.trust")}:</span>{" "}
              {trustStatusLabel(https.trustStatus, t)}
            </p>
            <p className="min-w-0 break-all sm:col-span-2">
              <span className="font-semibold text-stone-800">{t("settings.https.endpoint")}:</span>{" "}
              {https.proxyBaseUrl ?? proxyQuery.data?.base_url ?? t("settings.https.notAvailable")}
            </p>
            <p className="min-w-0 break-all sm:col-span-2">
              <span className="font-semibold text-stone-800">{t("settings.https.fingerprint")}:</span>{" "}
              {https.rootFingerprint ?? t("settings.https.notAvailable")}
            </p>
            <p className="sm:col-span-2">
              <span className="font-semibold text-stone-800">{t("settings.https.expires")}:</span>{" "}
              {https.expiresAt ?? t("settings.https.notAvailable")}
            </p>
            <p className="min-w-0 break-all sm:col-span-2">
              <span className="font-semibold text-stone-800">{t("settings.https.certificateDirectory")}:</span>{" "}
              {https.certificateDir}
            </p>
          </div>

          {https.trustStatus === "untrusted" ? (
            <div className="space-y-2 rounded-xl border border-amber-200 bg-amber-50 px-3 py-2 text-[12px] text-amber-900">
              <div className="flex items-center gap-1.5 font-semibold">
                <AlertTriangle className="h-3.5 w-3.5" />
                {t("settings.https.manualInstructions")}
              </div>
              {https.message ? <p>{https.message}</p> : null}
              {https.manualInstructions.map((instruction) => (
                <code className="block select-text break-all rounded-lg bg-white/80 px-2 py-1.5 text-[11px] text-amber-950" key={instruction}>
                  {instruction}
                </code>
              ))}
            </div>
          ) : https.message ? (
            <p className="rounded-xl border border-stone-200 bg-stone-50 px-3 py-2 text-[12px] text-stone-600">
              {https.message}
            </p>
          ) : null}

          <div className="flex flex-wrap items-center gap-2">
            {!https.certReady ? (
              <button
                className="inline-flex items-center gap-1.5 rounded-xl bg-stone-900 px-3 py-2 text-[13px] font-semibold text-white transition-colors hover:bg-stone-800 disabled:cursor-not-allowed disabled:opacity-60"
                disabled={isMutating}
                onClick={() => enableMutation.mutate()}
                type="button"
              >
                <ShieldCheck className="h-3.5 w-3.5" />
                {t("settings.https.generateAndImport")}
              </button>
            ) : null}
            <button
              className="inline-flex items-center gap-1.5 rounded-xl border border-stone-200 bg-white px-3 py-2 text-[13px] font-semibold text-stone-700 transition-colors hover:border-stone-300 disabled:cursor-not-allowed disabled:opacity-60"
              disabled={!https.certReady || isMutating}
              onClick={() => reimportMutation.mutate()}
              type="button"
            >
              <KeyRound className="h-3.5 w-3.5" />
              {t("settings.https.reimport")}
            </button>
            <button
              className="inline-flex items-center gap-1.5 rounded-xl border border-stone-200 bg-white px-3 py-2 text-[13px] font-semibold text-stone-700 transition-colors hover:border-stone-300 disabled:cursor-not-allowed disabled:opacity-60"
              disabled={!https.certReady || isMutating}
              onClick={() =>
                confirmAndRun(t("settings.https.regenerateConfirm"), () => regenerateMutation.mutate())
              }
              type="button"
            >
              <RotateCcw className="h-3.5 w-3.5" />
              {t("settings.https.regenerate")}
            </button>
            <button
              className="inline-flex items-center gap-1.5 rounded-xl border border-amber-200 bg-amber-50 px-3 py-2 text-[13px] font-semibold text-amber-800 transition-colors hover:border-amber-300 disabled:cursor-not-allowed disabled:opacity-60"
              disabled={!https.certReady || isMutating}
              onClick={() =>
                confirmAndRun(t("settings.https.uninstallConfirm"), () => uninstallMutation.mutate())
              }
              type="button"
            >
              <Unplug className="h-3.5 w-3.5" />
              {t("settings.https.uninstall")}
            </button>
            <button
              className="inline-flex items-center gap-1.5 rounded-xl border border-red-200 bg-red-50 px-3 py-2 text-[13px] font-semibold text-red-700 transition-colors hover:border-red-300 disabled:cursor-not-allowed disabled:opacity-60"
              disabled={!https.certReady || isMutating}
              onClick={() =>
                confirmAndRun(t("settings.https.deleteConfirm"), () => deleteMutation.mutate())
              }
              type="button"
            >
              <Trash2 className="h-3.5 w-3.5" />
              {t("settings.https.deleteCertificates")}
            </button>
            {isDesktop() ? (
              <button
                className="inline-flex items-center gap-1.5 rounded-xl border border-stone-200 bg-white px-3 py-2 text-[13px] font-semibold text-stone-700 transition-colors hover:border-stone-300 disabled:cursor-not-allowed disabled:opacity-60"
                disabled={!https.certReady || openDirectoryMutation.isPending}
                onClick={() => openDirectoryMutation.mutate()}
                type="button"
              >
                <FolderOpen className="h-3.5 w-3.5" />
                {t("settings.https.openDirectory")}
              </button>
            ) : null}
          </div>

          {mutationError ? (
            <p className="rounded-xl border border-red-200 bg-red-50 px-3 py-2 text-[12px] text-red-700">
              {actionError(mutationError)}
            </p>
          ) : null}
        </>
      )}
    </section>
  );
}
