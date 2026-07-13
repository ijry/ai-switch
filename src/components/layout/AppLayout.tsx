import { Settings } from "lucide-react";
import type { ReactNode } from "react";
import { supportedLanguages, useI18n, type Language } from "../../lib/i18n";

export type AgentPlatform =
  | "codex"
  | "claude"
  | "gemini"
  | "opencode"
  | "openclaw"
  | "hermes";

export const agentPlatforms: AgentPlatform[] = [
  "codex",
  "claude",
  "gemini",
  "opencode",
  "openclaw",
  "hermes",
];

export const agentScreenByPlatform: Record<AgentPlatform, string> = {
  codex: "Codex",
  claude: "Claude",
  gemini: "Gemini",
  opencode: "OpenCode",
  openclaw: "OpenClaw",
  hermes: "Hermes",
};

export const platformByAgentScreen: Record<string, AgentPlatform> = {
  Codex: "codex",
  Claude: "claude",
  Gemini: "gemini",
  OpenCode: "opencode",
  OpenClaw: "openclaw",
  Hermes: "hermes",
};

export const settingsFeatureScreens = [
  "Dashboard",
  "Providers",
  "Imports",
  "Library",
  "Targets",
  "Routing",
  "MCP",
  "Instances",
  "Wakeups",
  "Bulk",
  "Sync",
  "Sessions",
  "Updates",
  "Log",
] as const;

type AppLayoutProps = {
  children: ReactNode;
  activeScreen: string;
  onNavigate: (screen: string) => void;
  onLanguageChange?: (language: Language) => void;
  languageSaving?: boolean;
};

type AgentNavItem = {
  screen: string;
  platform: AgentPlatform;
  labelKey:
    | "nav.agent.codex"
    | "nav.agent.claude"
    | "nav.agent.gemini"
    | "nav.agent.opencode"
    | "nav.agent.openclaw"
    | "nav.agent.hermes";
};

const agentItems: AgentNavItem[] = [
  { screen: "Codex", platform: "codex", labelKey: "nav.agent.codex" },
  { screen: "Claude", platform: "claude", labelKey: "nav.agent.claude" },
  { screen: "Gemini", platform: "gemini", labelKey: "nav.agent.gemini" },
  { screen: "OpenCode", platform: "opencode", labelKey: "nav.agent.opencode" },
  { screen: "OpenClaw", platform: "openclaw", labelKey: "nav.agent.openclaw" },
  { screen: "Hermes", platform: "hermes", labelKey: "nav.agent.hermes" },
];

function isSettingsArea(screen: string) {
  return screen === "Settings" || (settingsFeatureScreens as readonly string[]).includes(screen);
}

function NavButton({
  label,
  active,
  onClick,
  variant = "standard",
}: {
  label: string;
  active: boolean;
  onClick: () => void;
  variant?: "primary" | "standard";
}) {
  const baseClasses =
    "group flex w-full items-center justify-between rounded-2xl border px-4 py-3 text-left transition focus:outline-none focus-visible:ring-2 focus-visible:ring-amber-500";
  const activeClasses =
    variant === "primary"
      ? "border-amber-500 bg-stone-950 text-white shadow-xl shadow-amber-900/20"
      : "border-stone-900 bg-stone-900 text-white shadow-lg shadow-stone-900/10";
  const idleClasses =
    variant === "primary"
      ? "border-amber-300/70 bg-amber-100/80 text-stone-950 hover:-translate-y-0.5 hover:bg-amber-200"
      : "border-stone-200 bg-white/70 text-stone-700 hover:-translate-y-0.5 hover:border-stone-300 hover:bg-white";

  return (
    <button
      aria-current={active ? "page" : undefined}
      className={`${baseClasses} ${active ? activeClasses : idleClasses}`}
      onClick={onClick}
      type="button"
    >
      <span className="flex items-center gap-3">
        <span
          className={`h-2.5 w-2.5 rounded-full ${
            active ? "bg-amber-300" : "bg-stone-300 group-hover:bg-amber-400"
          }`}
        />
        <span className="text-sm font-semibold">{label}</span>
      </span>
      <span className={active ? "text-amber-200" : "text-stone-300"}>→</span>
    </button>
  );
}

export function AppLayout({
  children,
  activeScreen,
  onNavigate,
  onLanguageChange,
  languageSaving = false,
}: AppLayoutProps) {
  const { language, setLanguage, t } = useI18n();
  const settingsActive = isSettingsArea(activeScreen);

  const handleLanguageChange = (nextLanguage: Language) => {
    if (onLanguageChange) {
      onLanguageChange(nextLanguage);
      return;
    }

    setLanguage(nextLanguage);
  };

  return (
    <main className="min-h-screen overflow-hidden bg-[radial-gradient(circle_at_top_left,_rgba(245,158,11,0.22),_transparent_34%),radial-gradient(circle_at_80%_10%,_rgba(20,83,45,0.16),_transparent_28%),linear-gradient(135deg,_#f7f1e5_0%,_#ece1cf_45%,_#e4ece4_100%)] px-4 py-5 text-stone-950 sm:px-6 lg:px-8">
      <div className="mx-auto flex max-w-7xl flex-col gap-5">
        <header className="flex flex-col gap-5 rounded-[2rem] border border-white/70 bg-white/60 p-5 shadow-2xl shadow-stone-900/5 backdrop-blur-xl sm:flex-row sm:items-center sm:justify-between">
          <div className="flex items-center gap-4">
            <div className="grid h-14 w-14 place-items-center rounded-2xl bg-stone-950 text-lg font-black tracking-tight text-amber-200 shadow-xl shadow-stone-950/20">
              AS
            </div>
            <div>
              <p className="text-xs font-bold uppercase tracking-[0.32em] text-amber-700">
                {t("layout.brandBadge")}
              </p>
              <h1 className="mt-1 text-3xl font-black tracking-tight text-stone-950">
                AI Switch
              </h1>
              <p className="mt-1 max-w-2xl text-sm text-stone-600">
                {t("layout.brandSubtitle")}
              </p>
            </div>
          </div>

          <label className="flex min-w-48 items-center justify-between gap-3 rounded-2xl border border-stone-200 bg-white/80 px-4 py-3 text-sm font-semibold text-stone-700 shadow-sm">
            <span>{t("layout.language")}</span>
            <select
              aria-label={t("layout.language")}
              className="rounded-xl border border-stone-200 bg-stone-50 px-3 py-2 text-sm font-semibold text-stone-900 outline-none transition focus:border-amber-500 focus:ring-2 focus:ring-amber-200"
              disabled={languageSaving}
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
        </header>

        <div className="grid gap-6 lg:grid-cols-[288px_minmax(0,1fr)]">
          <aside className="space-y-4">
            <section className="rounded-[2rem] border border-stone-950/10 bg-stone-950 p-4 text-white shadow-2xl shadow-stone-900/20">
              <p className="px-1 text-xs font-bold uppercase tracking-[0.22em] text-amber-200/80">
                {t("layout.agents")}
              </p>
              <div className="mt-3 space-y-2">
                {agentItems.map((item) => (
                  <NavButton
                    active={activeScreen === item.screen}
                    key={item.screen}
                    label={t(item.labelKey)}
                    onClick={() => onNavigate(item.screen)}
                    variant="primary"
                  />
                ))}
              </div>
            </section>

            <section className="rounded-[2rem] border border-stone-200 bg-white/70 p-4 shadow-sm">
              <p className="px-1 text-xs font-bold uppercase tracking-[0.22em] text-stone-500">
                {t("layout.system")}
              </p>
              <div className="mt-3">
                <NavButton
                  active={settingsActive}
                  label={t("nav.settings")}
                  onClick={() => onNavigate("Settings")}
                />
              </div>
              <p className="mt-3 flex items-start gap-2 rounded-2xl bg-stone-50 px-3 py-2 text-xs leading-5 text-stone-500">
                <Settings className="mt-0.5 h-3.5 w-3.5 shrink-0" />
                {t("layout.settingsHint")}
              </p>
            </section>
          </aside>

          <section className="min-w-0 animate-shell-in">{children}</section>
        </div>
      </div>
    </main>
  );
}
