import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ExternalLink, RefreshCcw, Unplug } from "lucide-react";
import { disconnectTailscale, getTailscaleStatus, startTailscaleLogin } from "../../lib/api/client";
import { useI18n } from "../../lib/i18n";

type TailscaleSettingsProps = {
  enabled: boolean;
};

export function TailscaleSettings({ enabled }: TailscaleSettingsProps) {
  const queryClient = useQueryClient();
  const { t } = useI18n();
  const statusQuery = useQuery({
    queryKey: ["tailscale-status"],
    queryFn: getTailscaleStatus,
  });

  const loginMutation = useMutation({
    mutationFn: startTailscaleLogin,
    onSuccess: (result) => {
      void statusQuery.refetch();
      if (result.loginUrl && typeof window !== "undefined") {
        window.open(result.loginUrl, "_blank", "noopener,noreferrer");
      }
    },
  });

  const disconnectMutation = useMutation({
    mutationFn: disconnectTailscale,
    onSuccess: (status) => {
      queryClient.setQueryData(["tailscale-status"], status);
    },
  });

  const status = statusQuery.data;

  return (
    <div className="space-y-3 rounded-2xl border border-stone-200 bg-stone-50/80 p-3">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <h3 className="text-[13px] font-semibold text-stone-950">{t("settings.tailscale.title")}</h3>
          <p className="text-[12px] text-stone-500">{t("settings.tailscale.subtitle")}</p>
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
        <p className="font-medium text-stone-900">
          {status?.state === "connected" ? t("settings.tailscale.connected") : t("settings.tailscale.notConnected")}
        </p>
        {status?.deviceName && <p>{t("settings.tailscale.device", { value: status.deviceName })}</p>}
        {status?.tailnetIp && <p>{t("settings.tailscale.ip", { value: status.tailnetIp })}</p>}
        {status?.message && <p className="text-stone-500">{status.message}</p>}
      </div>

      <div className="flex flex-wrap gap-2">
        <button
          className="inline-flex items-center gap-1.5 rounded-xl bg-stone-900 px-3 py-2 text-[13px] font-semibold text-white transition-colors hover:bg-stone-800 disabled:cursor-not-allowed disabled:opacity-60"
          disabled={loginMutation.isPending}
          onClick={() => loginMutation.mutate()}
          type="button"
        >
          <ExternalLink className="h-3.5 w-3.5" />
          {t("settings.tailscale.login")}
        </button>
        <button
          className="inline-flex items-center gap-1.5 rounded-xl border border-stone-200 bg-white px-3 py-2 text-[13px] font-semibold text-stone-700 transition-colors hover:border-stone-300 disabled:cursor-not-allowed disabled:opacity-60"
          disabled={disconnectMutation.isPending}
          onClick={() => disconnectMutation.mutate()}
          type="button"
        >
          <Unplug className="h-3.5 w-3.5" />
          {t("settings.tailscale.disconnect")}
        </button>
      </div>

      {loginMutation.data?.message && <p className="text-[12px] text-stone-500">{loginMutation.data.message}</p>}
      {loginMutation.isError && <p className="text-[12px] text-red-700">{t("settings.tailscale.loginError")}</p>}
    </div>
  );
}
