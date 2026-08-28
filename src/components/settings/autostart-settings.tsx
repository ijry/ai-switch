import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useI18n } from "../../lib/i18n";
import {
  disableAutostart,
  enableAutostart,
  isAutostartEnabled,
} from "../../lib/autostart";
import { isDesktop } from "../../lib/transport";

const AUTOSTART_QUERY_KEY = ["autostart"] as const;

export function AutostartSettings() {
  const desktop = isDesktop();
  const { t } = useI18n();
  const queryClient = useQueryClient();
  const stateQuery = useQuery({
    queryKey: AUTOSTART_QUERY_KEY,
    queryFn: isAutostartEnabled,
    enabled: desktop,
    retry: false,
    refetchOnWindowFocus: false,
  });
  const updateMutation = useMutation({
    mutationFn: (enabled: boolean) => (enabled ? enableAutostart() : disableAutostart()),
    onSuccess: (_result, enabled) => {
      queryClient.setQueryData(AUTOSTART_QUERY_KEY, enabled);
    },
  });

  if (!desktop) {
    return null;
  }

  return (
    <div className="grid gap-1">
      <label className="flex max-w-xl items-start gap-2 rounded-xl border border-stone-200 bg-white px-3 py-2.5 text-[12px] font-semibold text-stone-700">
        <input
          aria-label={t("settings.autostart.label")}
          checked={stateQuery.data === true}
          className="mt-0.5"
          disabled={stateQuery.isPending || stateQuery.isError || updateMutation.isPending}
          onChange={(event) => updateMutation.mutate(event.target.checked)}
          type="checkbox"
        />
        <span className="grid gap-1">
          <span>{t("settings.autostart.label")}</span>
          <span className="text-[11px] font-medium text-stone-500">
            {t("settings.autostart.description")}
          </span>
        </span>
      </label>
      {stateQuery.isError ? (
        <p className="text-[11px] font-medium text-red-700">{t("settings.autostart.readError")}</p>
      ) : null}
      {updateMutation.isError ? (
        <p className="text-[11px] font-medium text-red-700">{t("settings.autostart.updateError")}</p>
      ) : null}
    </div>
  );
}
