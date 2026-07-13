import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Layers3,
  Library,
  Network,
  Plug,
  RefreshCw,
  Route,
  Server,
  Settings2,
  TimerReset,
  UploadCloud,
} from "lucide-react";
import type { ComponentType } from "react";
import { getSettings, saveSettings } from "../lib/api/client";
import { normalizeLanguage, supportedLanguages, useI18n, type Language } from "../lib/i18n";

type FeatureEntry = {
  screen: string;
  titleKey:
    | "nav.dashboard"
    | "nav.library"
    | "nav.routing"
    | "nav.mcp"
    | "nav.instances"
    | "nav.wakeups"
    | "nav.bulk"
    | "nav.sync"
    | "nav.sessions"
    | "nav.updates"
    | "nav.log";
  descriptionKey:
    | "settings.feature.dashboard"
    | "settings.feature.library"
    | "settings.feature.routing"
    | "settings.feature.mcp"
    | "settings.feature.instances"
    | "settings.feature.wakeups"
    | "settings.feature.bulk"
    | "settings.feature.sync"
    | "settings.feature.sessions"
    | "settings.feature.updates"
    | "settings.feature.log";
  icon: ComponentType<{ className?: string }>;
};

// Keep only secondary tools in the settings hub.
// Providers / Imports / Targets map to agent-first flows, so they stay out of the UI.
const featureEntries: FeatureEntry[] = [
  {
    screen: "Dashboard",
    titleKey: "nav.dashboard",
    descriptionKey: "settings.feature.dashboard",
    icon: Layers3,
  },
  {
    screen: "Library",
    titleKey: "nav.library",
    descriptionKey: "settings.feature.library",
    icon: Library,
  },
  {
    screen: "Routing",
    titleKey: "nav.routing",
    descriptionKey: "settings.feature.routing",
    icon: Route,
  },
  {
    screen: "MCP",
    titleKey: "nav.mcp",
    descriptionKey: "settings.feature.mcp",
    icon: Plug,
  },
  {
    screen: "Instances",
    titleKey: "nav.instances",
    descriptionKey: "settings.feature.instances",
    icon: Server,
  },
  {
    screen: "Wakeups",
    titleKey: "nav.wakeups",
    descriptionKey: "settings.feature.wakeups",
    icon: TimerReset,
  },
  {
    screen: "Bulk",
    titleKey: "nav.bulk",
    descriptionKey: "settings.feature.bulk",
    icon: UploadCloud,
  },
  {
    screen: "Sync",
    titleKey: "nav.sync",
    descriptionKey: "settings.feature.sync",
    icon: RefreshCw,
  },
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
];

type SettingsScreenProps = {
  onOpenFeature?: (screen: string) => void;
};

export function SettingsScreen({ onOpenFeature }: SettingsScreenProps) {
  const queryClient = useQueryClient();
  const { language, setLanguage, t } = useI18n();
  const settingsQuery = useQuery({ queryKey: ["settings"], queryFn: getSettings });
  const saveMutation = useMutation({
    mutationFn: saveSettings,
    onSuccess: (settings) => {
      queryClient.setQueryData(["settings"], settings);
      setLanguage(normalizeLanguage(settings.language));
    },
  });

  if (settingsQuery.isLoading) {
    return <p className="text-steel">{t("settings.loading")}</p>;
  }

  if (!settingsQuery.data) {
    return <p className="text-ember">{t("settings.error")}</p>;
  }

  const settings = settingsQuery.data;
  const handleLanguageChange = (nextLanguage: Language) => {
    setLanguage(nextLanguage);
    saveMutation.mutate({ ...settings, language: nextLanguage });
  };

  return (
    <section className="space-y-5">
      <div className="rounded-[2rem] border border-stone-950/10 bg-stone-950 p-5 text-white shadow-2xl shadow-stone-950/15">
        <p className="text-xs font-black uppercase tracking-[0.3em] text-amber-200">
          {t("settings.hub.kicker")}
        </p>
        <h1 className="mt-3 text-3xl font-black tracking-tight">{t("settings.title")}</h1>
        <p className="mt-2 max-w-3xl text-sm leading-6 text-stone-300">
          {t("settings.hub.subtitle")}
        </p>
      </div>

      <div className="rounded-[2rem] border border-stone-200 bg-white/80 p-5 shadow-xl shadow-stone-900/5">
        <div className="flex items-center justify-between gap-3">
          <div>
            <h2 className="text-xl font-black text-stone-950">{t("settings.features.title")}</h2>
            <p className="mt-1 text-sm text-stone-500">{t("settings.features.subtitle")}</p>
          </div>
        </div>
        <div className="mt-4 grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
          {featureEntries.map((entry) => {
            const Icon = entry.icon;
            return (
              <button
                className="rounded-3xl border border-stone-200 bg-stone-50 px-4 py-4 text-left transition hover:-translate-y-0.5 hover:border-amber-300 hover:bg-amber-50"
                key={entry.screen}
                onClick={() => onOpenFeature?.(entry.screen)}
                type="button"
              >
                <div className="flex items-center gap-3">
                  <span className="grid h-10 w-10 place-items-center rounded-2xl bg-stone-950 text-amber-200">
                    <Icon className="h-4 w-4" />
                  </span>
                  <div>
                    <p className="text-sm font-black text-stone-950">{t(entry.titleKey)}</p>
                    <p className="mt-1 text-xs leading-5 text-stone-500">
                      {t(entry.descriptionKey)}
                    </p>
                  </div>
                </div>
              </button>
            );
          })}
        </div>
      </div>

      <div className="space-y-4 rounded-[2rem] border border-stone-200 bg-white/80 p-6 shadow-xl shadow-stone-900/5">
        <div>
          <h2 className="text-xl font-black text-stone-950">{t("settings.app.title")}</h2>
          <p className="mt-1 text-sm text-stone-500">{t("settings.app.subtitle")}</p>
        </div>
        <p className="text-sm text-stone-600">{t("settings.dataDir", { path: settings.data_dir })}</p>
        <label className="flex max-w-sm flex-col gap-2 text-sm font-semibold text-stone-800">
          <span>{t("settings.language")}</span>
          <select
            aria-label={t("settings.language")}
            className="rounded-2xl border border-stone-200 bg-white px-4 py-3 text-sm font-semibold text-stone-900 shadow-sm outline-none transition focus:border-amber-500 focus:ring-2 focus:ring-amber-200"
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
          className="rounded-full bg-stone-950 px-4 py-2 text-sm font-semibold text-white transition-colors duration-200 hover:bg-stone-800"
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
        {saveMutation.data && <p className="text-emerald-700">{t("settings.saved")}</p>}
        {saveMutation.error && <p className="text-red-700">{t("settings.saveError")}</p>}
      </div>
    </section>
  );
}
