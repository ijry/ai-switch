import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Copy, ExternalLink, KeyRound, RefreshCcw, Unplug } from "lucide-react";
import {
  disconnectTailscale,
  getTailscaleStatus,
  startTailscaleLogin,
  startTailscaleWithAuthKey,
} from "../../lib/api/client";
import { useI18n } from "../../lib/i18n";

type TailscaleSettingsProps = {
  enabled: boolean;
  exposureMode?: "private" | "public";
};

const STATUS_LABEL_KEYS = {
  connected: "settings.tailscale.connected",
  disabled: "settings.tailscale.disabledState",
  needsLogin: "settings.tailscale.needsLogin",
  error: "settings.tailscale.error",
  connecting: "settings.tailscale.connecting",
  notConnected: "settings.tailscale.notConnected",
} as const;

function statusLabelKey(state: string | undefined): (typeof STATUS_LABEL_KEYS)[keyof typeof STATUS_LABEL_KEYS] {
  if (state && state in STATUS_LABEL_KEYS) {
    return STATUS_LABEL_KEYS[state as keyof typeof STATUS_LABEL_KEYS];
  }
  return STATUS_LABEL_KEYS.notConnected;
}

export function TailscaleSettings({ enabled, exposureMode = "private" }: TailscaleSettingsProps) {
  const queryClient = useQueryClient();
  const { t } = useI18n();
  const [authKey, setAuthKey] = useState("");
  const [copiedUrl, setCopiedUrl] = useState<string | null>(null);

  const statusQuery = useQuery({
    queryKey: ["tailscale-status"],
    queryFn: getTailscaleStatus,
  });

  const loginMutation = useMutation({
    mutationFn: startTailscaleLogin,
    onSuccess: (result) => {
      void statusQuery.refetch();
      // Desktop opens the browser from Rust. Keep a web fallback only.
      if (
        result.loginUrl &&
        typeof window !== "undefined" &&
        !(window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__
      ) {
        window.open(result.loginUrl, "_blank", "noopener,noreferrer");
      }
    },
  });

  const authKeyMutation = useMutation({
    mutationFn: (key: string) => startTailscaleWithAuthKey(key),
    onSuccess: (status) => {
      setAuthKey("");
      queryClient.setQueryData(["tailscale-status"], status);
    },
  });

  const disconnectMutation = useMutation({
    mutationFn: disconnectTailscale,
    onSuccess: (status) => {
      queryClient.setQueryData(["tailscale-status"], status);
    },
  });

  const status = statusQuery.data;
  const accessUrls = status?.accessUrls?.filter(Boolean) ?? [];
  const busy =
    loginMutation.isPending || authKeyMutation.isPending || disconnectMutation.isPending;

  const copyUrl = async (url: string) => {
    if (typeof navigator === "undefined" || !navigator.clipboard) {
      return;
    }
    await navigator.clipboard.writeText(url);
    setCopiedUrl(url);
  };

  return (
    <div className="space-y-3 rounded-2xl border border-stone-200 bg-stone-50/80 p-3">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <h3 className="text-[13px] font-semibold text-stone-950">{t("settings.tailscale.title")}</h3>
          <p className="text-[12px] text-stone-500">
            {exposureMode === "public"
              ? t("settings.tailscale.subtitlePublic")
              : t("settings.tailscale.subtitle")}
          </p>
        </div>
        <button
          className="inline-flex items-center gap-1.5 rounded-xl border border-stone-200 bg-white px-2.5 py-1.5 text-[12px] font-semibold text-stone-700 hover:border-stone-300"
          onClick={() => void statusQuery.refetch()}
          type="button"
        >
          <RefreshCcw className="h-3.5 w-3.5" />
          {t("settings.tailscale.refresh")}
        </button>
      </div>

      {!enabled ? (
        <p className="rounded-xl border border-amber-200 bg-amber-50 px-3 py-2 text-[12px] text-amber-800">
          {t("settings.tailscale.disabled")}
        </p>
      ) : null}

      <div className="rounded-xl border border-stone-200 bg-white px-3 py-2 text-[12px] text-stone-600">
        <div className="flex flex-wrap items-center gap-2">
          <span
            className={`inline-flex rounded-full px-2 py-0.5 text-[11px] font-semibold ${
              status?.state === "connected"
                ? "bg-emerald-50 text-emerald-700"
                : status?.state === "error"
                  ? "bg-red-50 text-red-700"
                  : "bg-stone-100 text-stone-700"
            }`}
          >
            {t(statusLabelKey(status?.state))}
          </span>
          {status?.serving ? (
            <span className="inline-flex rounded-full bg-sky-50 px-2 py-0.5 text-[11px] font-semibold text-sky-700">
              {t("settings.tailscale.serving")}
            </span>
          ) : null}
          {enabled ? (
            <span
              className={`inline-flex rounded-full px-2 py-0.5 text-[11px] font-semibold ${
                exposureMode === "public" || status?.public
                  ? "bg-amber-50 text-amber-800"
                  : "bg-stone-100 text-stone-700"
              }`}
            >
              {exposureMode === "public" || status?.public
                ? t("settings.tailscale.modePublic")
                : t("settings.tailscale.modePrivate")}
            </span>
          ) : null}
        </div>
        {status?.deviceName ? <p className="mt-1">{t("settings.tailscale.device", { value: status.deviceName })}</p> : null}
        {status?.tailnetIp ? <p>{t("settings.tailscale.ip", { value: status.tailnetIp })}</p> : null}
        {status?.magicDnsName ? (
          <p>{t("settings.tailscale.magicDns", { value: status.magicDnsName })}</p>
        ) : null}
        {status?.message ? <p className="text-stone-500">{status.message}</p> : null}
        {status?.state === "error" && status.message?.includes("built-in network component") ? (
          <p className="mt-1 text-red-700">{t("settings.tailscale.componentMissing")}</p>
        ) : null}
      </div>

      {enabled && accessUrls.length > 0 ? (
        <div className="space-y-2 rounded-xl border border-stone-200 bg-white px-3 py-2">
          <p className="text-[12px] font-semibold text-stone-800">{t("settings.tailscale.remoteAccess")}</p>
          <ul className="space-y-1.5">
            {accessUrls.map((url) => (
              <li
                className="flex items-center justify-between gap-2 rounded-lg bg-stone-50 px-2.5 py-1.5 text-[12px] text-stone-700"
                key={url}
              >
                <span className="min-w-0 truncate font-mono">{url}</span>
                <button
                  className="inline-flex shrink-0 items-center gap-1 rounded-lg border border-stone-200 bg-white px-2 py-1 text-[11px] font-semibold text-stone-700 hover:border-stone-300"
                  onClick={() => void copyUrl(url)}
                  type="button"
                >
                  <Copy className="h-3 w-3" />
                  {copiedUrl === url ? t("settings.tailscale.copied") : t("settings.tailscale.copyUrl")}
                </button>
              </li>
            ))}
          </ul>
        </div>
      ) : null}

      {enabled ? (
        <div className="space-y-2">
          <label className="flex flex-col gap-1.5 text-[12px] font-medium text-stone-600">
            <span>{t("settings.tailscale.authKey")}</span>
            <div className="flex flex-wrap gap-2">
              <input
                aria-label={t("settings.tailscale.authKey")}
                autoComplete="off"
                className="min-w-0 flex-1 rounded-xl border border-stone-200 bg-white px-3 py-2 text-[13px] text-stone-900 outline-none transition focus:border-stone-400 focus:ring-2 focus:ring-stone-100"
                onChange={(event) => setAuthKey(event.target.value)}
                placeholder={t("settings.tailscale.authKeyPlaceholder")}
                type="password"
                value={authKey}
              />
              <button
                className="inline-flex items-center gap-1.5 rounded-xl bg-stone-900 px-3 py-2 text-[13px] font-semibold text-white transition-colors hover:bg-stone-800 disabled:cursor-not-allowed disabled:opacity-60"
                disabled={busy || authKey.trim().length === 0}
                onClick={() => authKeyMutation.mutate(authKey.trim())}
                type="button"
              >
                <KeyRound className="h-3.5 w-3.5" />
                {t("settings.tailscale.connectAuthKey")}
              </button>
            </div>
          </label>
          {authKeyMutation.isError ? (
            <p className="text-[12px] text-red-700">{t("settings.tailscale.authKeyError")}</p>
          ) : null}
        </div>
      ) : null}

      <div className="flex flex-wrap gap-2">
        <button
          className="inline-flex items-center gap-1.5 rounded-xl bg-stone-900 px-3 py-2 text-[13px] font-semibold text-white transition-colors hover:bg-stone-800 disabled:cursor-not-allowed disabled:opacity-60"
          disabled={!enabled || busy}
          onClick={() => loginMutation.mutate()}
          type="button"
        >
          <ExternalLink className="h-3.5 w-3.5" />
          {t("settings.tailscale.login")}
        </button>
        <button
          className="inline-flex items-center gap-1.5 rounded-xl border border-stone-200 bg-white px-3 py-2 text-[13px] font-semibold text-stone-700 transition-colors hover:border-stone-300 disabled:cursor-not-allowed disabled:opacity-60"
          disabled={!enabled || busy}
          onClick={() => disconnectMutation.mutate()}
          type="button"
        >
          <Unplug className="h-3.5 w-3.5" />
          {t("settings.tailscale.disconnect")}
        </button>
      </div>

      {loginMutation.data?.message ? (
        <p className="text-[12px] text-stone-500">{loginMutation.data.message}</p>
      ) : null}
      {loginMutation.data?.loginUrl ? (
        <a
          className="inline-flex items-center gap-1 text-[12px] font-semibold text-sky-700 hover:text-sky-800"
          href={loginMutation.data.loginUrl}
          rel="noreferrer"
          target="_blank"
        >
          <ExternalLink className="h-3.5 w-3.5" />
          {loginMutation.data.loginUrl}
        </a>
      ) : null}
      {loginMutation.isError ? <p className="text-[12px] text-red-700">{t("settings.tailscale.loginError")}</p> : null}
      {enabled && status?.state === "connected" && accessUrls.length === 0 ? (
        <p className="text-[12px] text-amber-800">{t("settings.tailscale.webRequired")}</p>
      ) : null}
    </div>
  );
}

