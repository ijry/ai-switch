import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Images } from "lucide-react";
import { saveSettings } from "../../lib/api/client";
import type { AppSettingsView } from "../../lib/api/types";
import { useI18n } from "../../lib/i18n";
import { SaasPluginSwitch } from "../../saas";
import { SaasLocaleProvider } from "../../saas/i18n";

type PluginManagementSettingsProps = {
  settings: AppSettingsView;
  onImageGenerationEnabledChange?: (enabled: boolean) => void;
  onSaasConfigChanged?: () => void;
};

export function PluginManagementSettings({
  settings,
  onImageGenerationEnabledChange,
  onSaasConfigChanged,
}: PluginManagementSettingsProps) {
  const queryClient = useQueryClient();
  const { language, t } = useI18n();
  const saasLocale = language === "zh-CN" ? "zh" : "en";
  const imageGenerationMutation = useMutation({
    mutationFn: (enabled: boolean) =>
      saveSettings({ ...settings, image_generation_enabled: enabled }),
    onSuccess: (saved) => {
      queryClient.setQueryData(["settings"], saved);
      onImageGenerationEnabledChange?.(saved.image_generation_enabled);
    },
  });

  return (
    <section className="space-y-3 rounded-2xl border border-stone-200 bg-white/82 p-4 shadow-sm">
      <div className="flex items-start gap-3">
        <span className="grid h-8 w-8 shrink-0 place-items-center rounded-xl bg-stone-950 text-white">
          <Images className="h-4 w-4" />
        </span>
        <div>
          <h2 className="text-[15px] font-semibold text-stone-950">
            {t("settings.plugins.title")}
          </h2>
          <p className="text-[12px] text-stone-500">
            {t("settings.plugins.subtitle")}
          </p>
        </div>
      </div>

      <div className="grid gap-3 lg:grid-cols-2">
        <div className="rounded-xl border border-stone-200 bg-stone-50/70 p-3">
          <SaasLocaleProvider locale={saasLocale}>
            <SaasPluginSwitch onConfigChanged={onSaasConfigChanged} />
          </SaasLocaleProvider>
        </div>
        <label className="flex items-start gap-3 rounded-xl border border-stone-200 bg-white p-3 text-[12px] font-semibold text-stone-700">
          <input
            aria-label={t("settings.plugins.imageGeneration")}
            checked={settings.image_generation_enabled}
            className="mt-0.5"
            disabled={imageGenerationMutation.isPending}
            onChange={(event) => imageGenerationMutation.mutate(event.target.checked)}
            type="checkbox"
          />
          <span className="grid gap-1">
            <span>{t("settings.plugins.imageGeneration")}</span>
            <span className="text-[11px] font-medium text-stone-500">
              {t("settings.plugins.imageGenerationHint")}
            </span>
          </span>
        </label>
      </div>
      {imageGenerationMutation.isError ? (
        <p className="text-[12px] text-red-700">
          {t("settings.plugins.imageGenerationError")}
        </p>
      ) : null}
    </section>
  );
}
