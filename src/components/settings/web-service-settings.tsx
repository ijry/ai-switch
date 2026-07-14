import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { CircleStop, Server, ShieldCheck, RefreshCcw } from "lucide-react";
import {
  getWebServerStatus,
  getWebServiceConfig,
  saveWebServiceConfig,
  startWebServer,
  stopWebServer,
} from "../../lib/api/client";
import type { WebServiceConfig } from "../../lib/api/types";
import { useI18n } from "../../lib/i18n";
import { TailscaleSettings } from "./tailscale-settings";

const defaultConfig: WebServiceConfig = {
  host: "127.0.0.1",
  port: 3090,
  token: "",
  autoStart: false,
  tailscaleEnabled: false,
};

function normalizeConfig(config: WebServiceConfig): WebServiceConfig {
  return {
    host: config.host.trim() || defaultConfig.host,
    port: Number.isFinite(config.port) && config.port > 0 ? config.port : defaultConfig.port,
    token: config.token?.trim() || "",
    autoStart: Boolean(config.autoStart),
    tailscaleEnabled: Boolean(config.tailscaleEnabled),
    tailscaleHostname: config.tailscaleHostname?.trim() || null,
    tailscaleAuthKeyPresent: Boolean(config.tailscaleAuthKeyPresent),
  };
}

export function WebServiceSettings() {
  const queryClient = useQueryClient();
  const { t } = useI18n();
  const configQuery = useQuery({
    queryKey: ["web-service-config"],
    queryFn: getWebServiceConfig,
  });
  const statusQuery = useQuery({
    queryKey: ["web-server-status"],
    queryFn: getWebServerStatus,
  });
  const [form, setForm] = useState<WebServiceConfig>(defaultConfig);

  useEffect(() => {
    if (configQuery.data) {
      setForm(normalizeConfig(configQuery.data));
    }
  }, [configQuery.data]);

  const saveMutation = useMutation({
    mutationFn: async () => {
      const saved = await saveWebServiceConfig(normalizeConfig(form));
      queryClient.setQueryData(["web-service-config"], saved);
      return saved;
    },
  });

  const startMutation = useMutation({
    mutationFn: async () => {
      const saved = await saveWebServiceConfig(normalizeConfig(form));
      queryClient.setQueryData(["web-service-config"], saved);
      const status = await startWebServer();
      queryClient.setQueryData(["web-server-status"], status);
      return status;
    },
  });

  const stopMutation = useMutation({
    mutationFn: async () => {
      const status = await stopWebServer();
      queryClient.setQueryData(["web-server-status"], status);
      return status;
    },
  });

  const status = statusQuery.data;

  return (
    <section className="space-y-3 rounded-2xl border border-stone-200 bg-white/82 p-4 shadow-sm">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <span className="grid h-8 w-8 place-items-center rounded-xl bg-stone-950 text-white">
              <Server className="h-4 w-4" />
            </span>
            <div>
              <h2 className="text-[15px] font-semibold text-stone-950">{t("settings.webService.title")}</h2>
              <p className="text-[12px] text-stone-500">{t("settings.webService.subtitle")}</p>
            </div>
          </div>
        </div>
        <button
          className="inline-flex items-center gap-1.5 rounded-xl border border-stone-200 bg-stone-50 px-3 py-2 text-[12px] font-semibold text-stone-700 transition-colors hover:border-stone-300 hover:bg-white"
          onClick={() => {
            void configQuery.refetch();
            void statusQuery.refetch();
          }}
          type="button"
        >
          <RefreshCcw className="h-3.5 w-3.5" />
          {t("settings.webService.refresh")}
        </button>
      </div>

      {configQuery.isLoading ? (
        <p className="text-[12px] text-stone-500">{t("settings.webService.loading")}</p>
      ) : (
        <>
          <div className="grid gap-3 sm:grid-cols-2">
            <label className="flex flex-col gap-1.5 text-[12px] font-medium text-stone-600">
              <span>{t("settings.webService.host")}</span>
              <input
                className="rounded-xl border border-stone-200 bg-white px-3 py-2 text-[13px] text-stone-900 outline-none transition focus:border-stone-400 focus:ring-2 focus:ring-stone-100"
                onChange={(event) => setForm((current) => ({ ...current, host: event.target.value }))}
                value={form.host}
              />
            </label>
            <label className="flex flex-col gap-1.5 text-[12px] font-medium text-stone-600">
              <span>{t("settings.webService.port")}</span>
              <input
                className="rounded-xl border border-stone-200 bg-white px-3 py-2 text-[13px] text-stone-900 outline-none transition focus:border-stone-400 focus:ring-2 focus:ring-stone-100"
                min={1}
                max={65535}
                onChange={(event) =>
                  setForm((current) => ({ ...current, port: Number(event.target.value) }))
                }
                type="number"
                value={form.port}
              />
            </label>
          </div>

          <label className="flex flex-col gap-1.5 text-[12px] font-medium text-stone-600">
            <span>{t("settings.webService.token")}</span>
            <input
              className="rounded-xl border border-stone-200 bg-white px-3 py-2 text-[13px] text-stone-900 outline-none transition focus:border-stone-400 focus:ring-2 focus:ring-stone-100"
              onChange={(event) => setForm((current) => ({ ...current, token: event.target.value }))}
              type="password"
              value={form.token ?? ""}
            />
          </label>

          <div className="flex flex-wrap gap-3">
            <label className="inline-flex items-center gap-2 rounded-xl border border-stone-200 bg-stone-50 px-3 py-2 text-[12px] font-medium text-stone-700">
              <input
                checked={form.autoStart}
                onChange={(event) =>
                  setForm((current) => ({ ...current, autoStart: event.target.checked }))
                }
                type="checkbox"
              />
              {t("settings.webService.autoStart")}
            </label>
            <label className="inline-flex items-center gap-2 rounded-xl border border-stone-200 bg-stone-50 px-3 py-2 text-[12px] font-medium text-stone-700">
              <input
                checked={form.tailscaleEnabled}
                onChange={(event) =>
                  setForm((current) => ({ ...current, tailscaleEnabled: event.target.checked }))
                }
                type="checkbox"
              />
              {t("settings.webService.tailscaleEnabled")}
            </label>
          </div>

          <div className="flex flex-wrap items-center gap-2">
            <button
              className="rounded-xl bg-stone-900 px-3 py-2 text-[13px] font-semibold text-white transition-colors hover:bg-stone-800 disabled:cursor-not-allowed disabled:opacity-60"
              disabled={saveMutation.isPending}
              onClick={() => saveMutation.mutate()}
              type="button"
            >
              {t("settings.webService.save")}
            </button>
            {status?.running ? (
              <button
                className="inline-flex items-center gap-1.5 rounded-xl border border-stone-200 bg-white px-3 py-2 text-[13px] font-semibold text-stone-700 transition-colors hover:border-stone-300 disabled:cursor-not-allowed disabled:opacity-60"
                disabled={stopMutation.isPending}
                onClick={() => stopMutation.mutate()}
                type="button"
              >
                <CircleStop className="h-3.5 w-3.5" />
                {t("settings.webService.stop")}
              </button>
            ) : (
              <button
                className="inline-flex items-center gap-1.5 rounded-xl border border-stone-200 bg-white px-3 py-2 text-[13px] font-semibold text-stone-700 transition-colors hover:border-stone-300 disabled:cursor-not-allowed disabled:opacity-60"
                disabled={startMutation.isPending}
                onClick={() => startMutation.mutate()}
                type="button"
              >
                <ShieldCheck className="h-3.5 w-3.5" />
                {t("settings.webService.start")}
              </button>
            )}
          </div>

          <div className="rounded-xl border border-stone-200 bg-stone-50 px-3 py-2 text-[12px] text-stone-600">
            {status?.running && status.baseUrl ? (
              <p>{t("settings.webService.running", { url: status.baseUrl })}</p>
            ) : (
              <p>{t("settings.webService.stopped")}</p>
            )}
            {saveMutation.isError && <p className="mt-1 text-red-700">{t("settings.webService.saveError")}</p>}
            {startMutation.isError && <p className="mt-1 text-red-700">{t("settings.webService.startError")}</p>}
            {stopMutation.isError && <p className="mt-1 text-red-700">{t("settings.webService.stopError")}</p>}
            {saveMutation.isSuccess && <p className="mt-1 text-emerald-700">{t("settings.webService.saved")}</p>}
          </div>

          <TailscaleSettings enabled={form.tailscaleEnabled} />
        </>
      )}
    </section>
  );
}
