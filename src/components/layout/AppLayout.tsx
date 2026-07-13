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
    "group flex w-full items-center justify-between rounded-xl border px-3 py-2 text-left text-[13px] transition-colors duration-150 focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-400";
  const activeClasses =
    variant === "primary"
      ? "border-stone-300 bg-white text-stone-950 shadow-sm"
      : "border-stone-300 bg-stone-100 text-stone-950 shadow-sm";
  const idleClasses =
    variant === "primary"
      ? "border-transparent bg-transparent text-stone-600 hover:bg-white/60 hover:text-stone-950"
      : "border-transparent bg-transparent text-stone-600 hover:bg-stone-100 hover:text-stone-950";

  return (
    <button
      aria-current={active ? "page" : undefined}
      className={`${baseClasses} ${active ? activeClasses : idleClasses}`}
      onClick={onClick}
      type="button"
    >
      <span className="flex min-w-0 items-center gap-2">
        <span
          className={`h-1.5 w-1.5 shrink-0 rounded-full ${
            active ? "bg-amber-500" : "bg-stone-300 group-hover:bg-stone-400"
          }`}
        />
        <span className="truncate font-medium">{label}</span>
      </span>
      <span className={active ? "text-stone-400" : "text-transparent"}>/</span>
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
    <main className="min-h-screen p-3 text-stone-950 sm:p-4">
      <div className="mx-auto h-[calc(100vh-1.5rem)] max-w-[1440px] overflow-hidden rounded-[22px] border border-white/70 bg-white/55 shadow-2xl shadow-stone-900/12 backdrop-blur-2xl sm:h-[calc(100vh-2rem)]">
        <div className="flex h-10 items-center justify-between border-b border-stone-200/75 bg-white/60 px-3">
          <div className="flex items-center gap-2">
            <span className="h-3 w-3 rounded-full bg-[#ff5f57]" />
            <span className="h-3 w-3 rounded-full bg-[#febc2e]" />
            <span className="h-3 w-3 rounded-full bg-[#28c840]" />
          </div>
          <div className="text-[13px] font-semibold text-stone-600">AI Switch</div>
          <label className="flex items-center gap-2 text-[12px] font-medium text-stone-500">
            <span>{t("layout.language")}</span>
            <select
              aria-label={t("layout.language")}
              className="rounded-lg border border-stone-200 bg-white/80 px-2 py-1 text-[12px] font-medium text-stone-800 outline-none transition focus:border-blue-400 focus:ring-2 focus:ring-blue-200"
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
        </div>

        <div className="grid h-[calc(100%-2.5rem)] grid-cols-1 lg:grid-cols-[220px_minmax(0,1fr)]">
          <aside className="min-h-0 border-b border-stone-200/75 bg-stone-100/70 p-3 lg:border-b-0 lg:border-r">
            <div className="mb-3 flex items-center gap-2 px-1">
              <div className="grid h-8 w-8 place-items-center rounded-xl bg-stone-900 text-[12px] font-black text-white shadow-sm">
                AS
              </div>
              <div className="min-w-0">
                <p className="truncate text-[13px] font-semibold text-stone-950">AI Switch</p>
                <p className="truncate text-[11px] text-stone-500">{t("layout.brandBadge")}</p>
              </div>
            </div>

            <div className="space-y-4">
              <section>
                <p className="px-2 pb-1 text-[11px] font-semibold uppercase tracking-wide text-stone-400">
                  {t("layout.agents")}
                </p>
                <div className="space-y-1">
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

              <section>
                <p className="px-2 pb-1 text-[11px] font-semibold uppercase tracking-wide text-stone-400">
                  {t("layout.system")}
                </p>
                <NavButton
                  active={settingsActive}
                  label={t("nav.settings")}
                  onClick={() => onNavigate("Settings")}
                />
              </section>
            </div>
          </aside>

          <section className="min-h-0 min-w-0 overflow-auto bg-white/50 p-3 sm:p-4">{children}</section>
        </div>
      </div>
    </main>
  );
}
