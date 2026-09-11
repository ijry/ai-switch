import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { motion } from "motion/react";
import {
  Bell,
  Layers3,
  LockKeyhole,
  Network,
  Puzzle,
  Server,
  Settings2,
} from "lucide-react";
import type { ComponentType } from "react";
import { getSettings, saveSettings } from "../lib/api/client";
import { normalizeLanguage, supportedLanguages, useI18n, type Language } from "../lib/i18n";
import { normalizeThemePreference, type ThemePreference } from "../lib/theme";
import { AutostartSettings } from "../components/settings/autostart-settings";
import { RouteProxyHttpsSettings } from "../components/settings/route-proxy-https-settings";
import { NotificationSettings } from "../components/settings/notification-settings";
import { WebServiceSettings } from "../components/settings/web-service-settings";
import { useState } from "react";import { MotionPresence } from "../components/motion/MotionPrimitives";
import {
  agentPlatforms,
  createDefaultAgentVisibility,
  type AgentPlatform,
  type AgentVisibility,
} from "../lib/agentVisibility";
import { PluginManagementSettings } from "../components/settings/plugin-management-settings";

type FeatureEntry = {
  screen?: string;
  section?: "webService" | "https" | "notification" | "plugins";
  titleKey:
    | "nav.sessions"
    | "nav.updates"
    | "nav.log"
    | "nav.webService"
    | "settings.https.title"
    | "settings.plugins.title"
    | "notification.title";
  descriptionKey:
    | "settings.feature.sessions"
    | "settings.feature.updates"
    | "settings.feature.log"
    | "settings.feature.webService"
    | "settings.feature.https"
    | "settings.feature.plugins"
    | "notification.subtitle";
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
  {
    section: "plugins",
    titleKey: "settings.plugins.title",
    descriptionKey: "settings.feature.plugins",
    icon: Puzzle,
  },
  {
    section: "notification",
    titleKey: "notification.title",
    descriptionKey: "notification.subtitle",
    icon: Bell,
  },
];

type SettingsScreenProps = {
  onOpenFeature?: (screen: string) => void;
  agentVisibility?: AgentVisibility;
  onAgentVisibilityChange?: (platform: AgentPlatform, visible: boolean) => void;
  onSaasConfigChanged?: () => void;
  onImageGenerationEnabledChange?: (enabled: boolean) => void;
};

const agentLabelKeys = {
  codex: "nav.agent.codex",
  claude: "nav.agent.claude",
  grok: "nav.agent.grok",
  gemini: "nav.agent.gemini",
  opencode: "nav.agent.opencode",
  openclaw: "nav.agent.openclaw",
  hermes: "nav.agent.hermes",
} as const;

const themeOptions: {
  value: ThemePreference;
  labelKey: "settings.theme.system" | "settings.theme.light" | "settings.theme.dark";
}[] = [
  { value: "system", labelKey: "settings.theme.system" },
  { value: "light", labelKey: "settings.theme.light" },
  { value: "dark", labelKey: "settings.theme.dark" },
];

export function SettingsScreen({
  onOpenFeature,
  agentVisibility,
  onAgentVisibilityChange,
  onSaasConfigChanged,
  onImageGenerationEnabledChange,
}: SettingsScreenProps) {
  const queryClient = useQueryClient();
  const { language, setLanguage, t } = useI18n();
  const [activeSection, setActiveSection] = useState<"webService" | "https" | "notification" | "plugins">("webService");
  const [localAgentVisibility, setLocalAgentVisibility] = useState(createDefaultAgentVisibility);
  // Proxy drafts as `null` = "follow the loaded settings"; they overlay the
  // settings while the user edits. Hooks stay above the loading early-returns.
  const [proxyEnabledDraft, setProxyEnabledDraft] = useState<boolean | null>(null);
  const [proxyUrlDraft, setProxyUrlDraft] = useState<string | null>(null);
  const [proxyError, setProxyError] = useState<string | null>(null);
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
  const effectiveAgentVisibility = agentVisibility ?? localAgentVisibility;
  const handleAgentVisibilityChange = (platform: AgentPlatform, visible: boolean) => {
    if (onAgentVisibilityChange) {
      onAgentVisibilityChange(platform, visible);
      return;
    }
    setLocalAgentVisibility((current) => ({ ...current, [platform]: visible }));
  };
  const handleLanguageChange = (nextLanguage: Language) => {
    setLanguage(nextLanguage);
    saveMutation.mutate({ ...settings, language: nextLanguage });
  };

  // Proxy fields: drafts overlay the loaded settings so the card edits smoothly
  // and resets cleanly if settings reload underneath.
  const PROXY_EXAMPLE = "http://127.0.0.1:7890";
  const proxyEnabled = proxyEnabledDraft ?? settings.proxy_enabled;
  const proxyUrl = proxyUrlDraft ?? settings.proxy_url ?? "";

  const saveProxy = (nextEnabled: boolean, nextUrl: string) => {
    if (nextEnabled && !nextUrl.trim()) {
      setProxyError(t("settings.proxy.required"));
      return;
    }
    setProxyError(null);
    saveMutation.mutate({
      ...settings,
      proxy_enabled: nextEnabled,
      proxy_url: nextUrl.trim() || null,
    });
  };

  const handleProxyEnabledChange = (next: boolean) => {
    setProxyEnabledDraft(next);
    if (next && !proxyUrl.trim()) {
      setProxyError(t("settings.proxy.required"));
      return;
    }
    setProxyError(null);
    saveProxy(next, proxyUrl);
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
                className="rounded-xl border border-stone-200 bg-stone-50/70 px-3 py-2.5 text-left motion-control hover:border-stone-300 hover:bg-white focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-400"
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

      <MotionPresence>
        {activeSection === "webService" && (
          <motion.div
            key="settings-web-service"
            initial={{ opacity: 0, y: 8 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -8 }}
            transition={{ duration: 0.24, ease: [0.22, 1, 0.36, 1] }}
          >
            <WebServiceSettings />
          </motion.div>
        )}
        {activeSection === "plugins" && (
          <PluginManagementSettings
            onImageGenerationEnabledChange={onImageGenerationEnabledChange}
            onSaasConfigChanged={onSaasConfigChanged}
            settings={settings}
          />
        )}
        {activeSection === "https" && (
          <motion.div
            key="settings-https"
            initial={{ opacity: 0, y: 8 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -8 }}
            transition={{ duration: 0.24, ease: [0.22, 1, 0.36, 1] }}
          >
            <RouteProxyHttpsSettings />
          </motion.div>
        )}
        {activeSection === "notification" && (
          <motion.div
            key="settings-notification"
            initial={{ opacity: 0, y: 8 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -8 }}
            transition={{ duration: 0.24, ease: [0.22, 1, 0.36, 1] }}
          >
            <NotificationSettings settings={settings} />
          </motion.div>
        )}
      </MotionPresence>

      <div className="space-y-3 rounded-2xl border border-stone-200 bg-white/82 p-4 shadow-sm">
        <h2 className="text-[15px] font-semibold text-stone-950">{t("settings.proxy.title")}</h2>
        <p className="text-[12px] text-stone-500">{t("settings.proxy.subtitle")}</p>
        <label className="flex max-w-xl items-start gap-2 rounded-xl border border-stone-200 bg-white px-3 py-2.5 text-[12px] font-semibold text-stone-700">
          <input
            aria-label={t("settings.proxy.enable")}
            checked={proxyEnabled}
            className="mt-0.5"
            disabled={saveMutation.isPending}
            onChange={(event) => handleProxyEnabledChange(event.target.checked)}
            type="checkbox"
          />
          <span>{t("settings.proxy.enable")}</span>
        </label>
        <label className="flex max-w-sm flex-col gap-1.5 text-[12px] font-semibold text-stone-600">
          <span>{t("settings.proxy.address")}</span>
          <input
            aria-label={t("settings.proxy.address")}
            className="rounded-xl border border-stone-200 bg-white px-3 py-2 text-[13px] font-medium text-stone-900 shadow-sm outline-none motion-control focus:border-blue-400 focus:ring-2 focus:ring-blue-100"
            disabled={saveMutation.isPending}
            onBlur={() => {
              if (proxyEnabled && !proxyUrl.trim()) {
                setProxyError(t("settings.proxy.required"));
                return;
              }
              setProxyError(null);
              if (proxyUrl.trim() !== (settings.proxy_url ?? "")) {
                saveProxy(proxyEnabled, proxyUrl);
              }
            }}
            onChange={(event) => {
              setProxyUrlDraft(event.target.value);
              if (event.target.value.trim()) setProxyError(null);
            }}
            placeholder={PROXY_EXAMPLE}
            type="text"
            value={proxyUrl}
          />
          {proxyError ? (
            <span className="text-[11px] font-medium text-red-700">{proxyError}</span>
          ) : (
            <span className="text-[11px] font-medium text-stone-500">{t("settings.proxy.hint")}</span>
          )}
        </label>
      </div>

      <div className="space-y-3 rounded-2xl border border-stone-200 bg-white/82 p-4 shadow-sm">
        <h2 className="text-[15px] font-semibold text-stone-950">{t("settings.app.title")}</h2>
        <div className="space-y-2 rounded-xl border border-stone-200 bg-stone-50/70 p-3">
          <div>
            <h3 className="text-[13px] font-semibold text-stone-800">{t("settings.agentVisibility.title")}</h3>
            <p className="mt-1 text-[11px] font-medium text-stone-500">{t("settings.agentVisibility.subtitle")}</p>
          </div>
          <div className="grid gap-2 sm:grid-cols-2">
            {agentPlatforms.map((platform) => (
              <label
                className="flex items-center gap-2 rounded-lg border border-stone-200 bg-white px-3 py-2 text-[12px] font-semibold text-stone-700"
                key={platform}
              >
                <input
                  aria-label={t("settings.agentVisibility.show", { agent: t(agentLabelKeys[platform]) })}
                  checked={effectiveAgentVisibility[platform]}
                  className="accent-blue-600"
                  onChange={(event) => handleAgentVisibilityChange(platform, event.target.checked)}
                  type="checkbox"
                />
                <span>{t("settings.agentVisibility.show", { agent: t(agentLabelKeys[platform]) })}</span>
              </label>
            ))}
          </div>
        </div>
        <p className="rounded-xl border border-stone-200 bg-stone-50 px-3 py-2 text-[12px] text-stone-600">
          {t("settings.dataDir", { path: settings.data_dir })}
        </p>
        <AutostartSettings />
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
            aria-label={t("settings.closeToTray.label")}
            checked={settings.close_to_tray}
            className="mt-0.5"
            disabled={saveMutation.isPending}
            onChange={(event) =>
              saveMutation.mutate({
                ...settings,
                close_to_tray: event.target.checked,
              })
            }
            type="checkbox"
          />
          <span className="grid gap-1">
            <span>{t("settings.closeToTray.label")}</span>
            <span className="text-[11px] font-medium text-stone-500">
              {t("settings.closeToTray.hint")}
            </span>
          </span>
        </label>
        <label className="flex max-w-sm flex-col gap-1.5 text-[12px] font-semibold text-stone-600">
          <span>{t("settings.language")}</span>
          <select
            aria-label={t("settings.language")}
            className="rounded-xl border border-stone-200 bg-white px-3 py-2 text-[13px] font-medium text-stone-900 shadow-sm outline-none motion-control focus:border-blue-400 focus:ring-2 focus:ring-blue-100"
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
        <label className="flex max-w-sm flex-col gap-1.5 text-[12px] font-semibold text-stone-600">
          <span>{t("settings.theme")}</span>
          <select
            aria-label={t("settings.theme")}
            className="rounded-xl border border-stone-200 bg-white px-3 py-2 text-[13px] font-medium text-stone-900 shadow-sm outline-none motion-control focus:border-blue-400 focus:ring-2 focus:ring-blue-100"
            disabled={saveMutation.isPending}
            onChange={(event) =>
              saveMutation.mutate({ ...settings, theme: event.target.value })
            }
            value={normalizeThemePreference(settings.theme)}
          >
            {themeOptions.map((option) => (
              <option key={option.value} value={option.value}>
                {t(option.labelKey)}
              </option>
            ))}
          </select>
        </label>
        {saveMutation.data && <p className="text-[13px] font-medium text-emerald-700">{t("settings.saved")}</p>}
        {saveMutation.error && <p className="text-[13px] font-medium text-red-700">{t("settings.saveError")}</p>}
      </div>
    </section>
  );
}
