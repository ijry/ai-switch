import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Layers3,
  LockKeyhole,
  Network,
  Server,
  Settings2,
} from "lucide-react";
import type { ComponentType } from "react";
import { getSettings, saveSettings } from "../lib/api/client";
import { normalizeLanguage, supportedLanguages, useI18n, type Language } from "../lib/i18n";
import { RouteProxyHttpsSettings } from "../components/settings/route-proxy-https-settings";
import { WebServiceSettings } from "../components/settings/web-service-settings";
import { useState } from "react";

type FeatureEntry = {
  screen?: string;
  section?: "webService" | "https";
  titleKey: "nav.sessions" | "nav.updates" | "nav.log" | "nav.webService" | "settings.https.title";
  descriptionKey:
    | "settings.feature.sessions"
    | "settings.feature.updates"
    | "settings.feature.log"
    | "settings.feature.webService"
    | "settings.feature.https";
  icon: ComponentType<{ className?: string }>;
};

// Keep only shipped utility entries here. Agent-facing workflows stay in the agent tabs.
const featureEntries: FeatureEntry[] = [
  {
    screen: "Sessions",
    titleKey: "nav.sessions",
    descriptionKey: "settings.feature.sessions",
    icon: Network,
  },
  {
    screen: "Updates",
    titleKey: "nav.updates",
    descriptionKey: "settings.feature.updates",
    icon: Settings2,
  },
  {
    screen: "Log",
    titleKey: "nav.log",
    descriptionKey: "settings.feature.log",
    icon: Layers3,
  },
  {
    section: "webService",
    titleKey: "nav.webService",
    descriptionKey: "settings.feature.webService",
    icon: Server,
  },
  {
    section: "https",
    titleKey: "settings.https.title",
    descriptionKey: "settings.feature.https",
    icon: LockKeyhole,
  },
];

type SettingsScreenProps = {
  onOpenFeature?: (screen: string) => void;
};

export function SettingsScreen({ onOpenFeature }: SettingsScreenProps) {
  const queryClient = useQueryClient();
  const { language, setLanguage, t } = useI18n();
  const [activeSection, setActiveSection] = useState<"webService" | "https">("webService");
  const settingsQuery = useQuery({ queryKey: ["settings"], queryFn: getSettings });
  const saveMutation = useMutation({
    mutationFn: saveSettings,
    onSuccess: (settings) => {
      queryClient.setQueryData(["settings"], settings);
      setLanguage(normalizeLanguage(settings.language));
    },
  });

  if (settingsQuery.isLoading) {
    return <p className="text-sm text-stone-500">{t("settings.loading")}</p>;
  }

  if (!settingsQuery.data) {
    return <p className="text-sm text-red-700">{t("settings.error")}</p>;
  }

  const settings = settingsQuery.data;
  const handleLanguageChange = (nextLanguage: Language) => {
    setLanguage(nextLanguage);
    saveMutation.mutate({ ...settings, language: nextLanguage });
  };

  return (
    <section className="space-y-3">
      <div className="rounded-2xl border border-stone-200 bg-white/82 shadow-sm">
        <div className="border-b border-stone-200 px-4 py-3">
          <p className="text-[11px] font-semibold uppercase tracking-wide text-stone-400">
            {t("settings.hub.kicker")}
          </p>
          <h1 className="mt-0.5 text-lg font-semibold tracking-tight text-stone-950">
            {t("settings.title")}
          </h1>
        </div>

        <div className="px-4 py-3">
          <h2 className="text-[15px] font-semibold text-stone-950">{t("settings.features.title")}</h2>
        </div>
        <div className="grid gap-2 px-3 pb-3 sm:grid-cols-2 xl:grid-cols-3">
          {featureEntries.map((entry) => {
            const Icon = entry.icon;
            return (
              <button
                className="rounded-xl border border-stone-200 bg-stone-50/70 px-3 py-2.5 text-left transition-colors hover:border-stone-300 hover:bg-white focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-400"
                key={entry.screen ?? entry.section}
                onClick={() => {
                  if (entry.screen) {
                    onOpenFeature?.(entry.screen);
                    return;
                  }
                  if (entry.section) {
                    setActiveSection(entry.section);
                  }
                }}
                type="button"
              >
                <div className="flex items-center gap-2.5">
                  <span className="grid h-8 w-8 place-items-center rounded-lg bg-white text-stone-700 shadow-sm ring-1 ring-stone-200">
                    <Icon className="h-3.5 w-3.5" />
                  </span>
                  <div className="min-w-0">
                    <p className="truncate text-[13px] font-semibold text-stone-950">{t(entry.titleKey)}</p>
                    <p className="mt-0.5 truncate text-[12px] text-stone-500">{t(entry.descriptionKey)}</p>
                  </div>
                </div>
              </button>
            );
          })}
        </div>
      </div>

      {activeSection === "webService" && <WebServiceSettings />}
      {activeSection === "https" && <RouteProxyHttpsSettings />}

      <div className="space-y-3 rounded-2xl border border-stone-200 bg-white/82 p-4 shadow-sm">
        <h2 className="text-[15px] font-semibold text-stone-950">{t("settings.app.title")}</h2>
        <p className="rounded-xl border border-stone-200 bg-stone-50 px-3 py-2 text-[12px] text-stone-600">
          {t("settings.dataDir", { path: settings.data_dir })}
        </p>
        <label className="flex max-w-xl items-start gap-2 rounded-xl border border-stone-200 bg-white px-3 py-2.5 text-[12px] font-semibold text-stone-700">
          <input
            aria-label={t("settings.ccswitch.label")}
            checked={settings.ccswitch_deeplink_compat_enabled}
            className="mt-0.5"
            disabled={settings.ccswitch_deeplink_compat_supported === false || saveMutation.isPending}
            onChange={(event) =>
              saveMutation.mutate({
                ...settings,
                ccswitch_deeplink_compat_enabled: event.target.checked,
              })
            }
            type="checkbox"
          />
          <span className="grid gap-1">
            <span>{t("settings.ccswitch.label")}</span>
            <span className="text-[11px] font-medium text-stone-500">
              {settings.ccswitch_deeplink_compat_supported !== false
                ? t("settings.ccswitch.warning")
                : t("settings.ccswitch.unsupported")}
            </span>
          </span>
        </label>
        <label className="flex max-w-xl items-start gap-2 rounded-xl border border-stone-200 bg-white px-3 py-2.5 text-[12px] font-semibold text-stone-700">
          <input
            aria-label={t("settings.incrementalStreaming.label")}
            checked={settings.incremental_streaming_enabled}
            className="mt-0.5"
            disabled={saveMutation.isPending}
            onChange={(event) =>
              saveMutation.mutate({
                ...settings,
                incremental_streaming_enabled: event.target.checked,
              })
            }
            type="checkbox"
          />
          <span className="grid gap-1">
            <span>{t("settings.incrementalStreaming.label")}</span>
            <span className="text-[11px] font-medium text-stone-500">
              {t("settings.incrementalStreaming.warning")}
            </span>
          </span>
        </label>
        <label className="flex max-w-sm flex-col gap-1.5 text-[12px] font-semibold text-stone-600">
          <span>{t("settings.language")}</span>
          <select
            aria-label={t("settings.language")}
            className="rounded-xl border border-stone-200 bg-white px-3 py-2 text-[13px] font-medium text-stone-900 shadow-sm outline-none transition focus:border-blue-400 focus:ring-2 focus:ring-blue-100"
            disabled={saveMutation.isPending}
            onChange={(event) => handleLanguageChange(event.target.value as Language)}
            value={language}
          >
            {supportedLanguages.map((option) => (
              <option key={option.code} value={option.code}>
                {option.label}
              </option>
            ))}
          </select>
        </label>
        <button
          type="button"
          className="w-fit rounded-xl bg-stone-900 px-3 py-2 text-[13px] font-semibold text-white transition-colors duration-150 hover:bg-stone-800"
          onClick={() =>
            saveMutation.mutate({
              ...settings,
              language,
              theme: settings.theme === "dark" ? "system" : "dark",
            })
          }
        >
          {t("settings.themeToggle")}
        </button>
        {saveMutation.data && <p className="text-[13px] font-medium text-emerald-700">{t("settings.saved")}</p>}
        {saveMutation.error && <p className="text-[13px] font-medium text-red-700">{t("settings.saveError")}</p>}
      </div>
    </section>
  );
}
