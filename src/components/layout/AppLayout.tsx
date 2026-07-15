import type { ReactNode } from "react";
import { TerminalSquare } from "lucide-react";
import { AiSwitchLogo } from "../brand/AiSwitchLogo";
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
  "Sessions",
  "Updates",
  "Log",
  "CryptoTools",
  "OCR",
] as const;

type AppLayoutProps = {
  children: ReactNode;
  activeScreen: string;
  onNavigate: (screen: string) => void;
  onOpenVibe?: () => void;
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
      <span aria-hidden="true" className={active ? "text-stone-400" : "text-transparent"}>
        /
      </span>
    </button>
  );
}

export function AppLayout({
  children,
  activeScreen,
  onNavigate,
  onOpenVibe,
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
    <main className="h-screen max-h-[100dvh] overflow-hidden text-stone-950">
      <div className="grid h-full min-h-0 grid-cols-1 lg:grid-cols-[236px_minmax(0,1fr)]">
        <aside className="relative flex h-full min-h-0 flex-col overflow-hidden border-b border-white/70 bg-gradient-to-br from-slate-50/92 via-emerald-50/74 to-amber-50/70 p-3 shadow-xl shadow-stone-900/5 backdrop-blur-2xl lg:border-b-0 lg:border-r lg:border-white/80">
          <div className="pointer-events-none absolute inset-0 bg-[radial-gradient(circle_at_20%_15%,rgba(16,185,129,0.18),transparent_34%),radial-gradient(circle_at_88%_8%,rgba(245,158,11,0.16),transparent_30%),linear-gradient(180deg,rgba(255,255,255,0.72),rgba(255,255,255,0.38))]" />
          <div className="relative flex min-h-0 flex-1 flex-col">
            <div className="mb-5 flex items-start justify-between gap-3 rounded-2xl border border-white/80 bg-white/56 p-3 shadow-sm backdrop-blur-xl">
              <div className="flex min-w-0 items-center gap-2">
                <AiSwitchLogo className="h-9 w-9 shrink-0 rounded-2xl shadow-sm" />
                <div className="min-w-0">
                  <p className="truncate text-[13px] font-semibold text-stone-950">AI Switch</p>
                  <p className="truncate text-[11px] text-stone-500">{t("layout.brandBadge")}</p>
                </div>
              </div>
              <button
                aria-label={t("layout.switchToVibe")}
                className="grid h-8 w-8 shrink-0 place-items-center rounded-xl border border-stone-200 bg-white/70 text-stone-600 shadow-sm transition-colors hover:border-stone-300 hover:bg-white hover:text-stone-950 focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-400"
                onClick={onOpenVibe}
                type="button"
              >
                <TerminalSquare className="h-4 w-4" />
              </button>
            </div>

            <label className="mb-5 flex items-center justify-between gap-2 rounded-2xl border border-white/70 bg-white/50 px-3 py-2 text-[12px] font-medium text-stone-500 backdrop-blur-xl">
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

            <div className="min-h-0 flex-1 space-y-4 overflow-y-auto pr-0.5">
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
                  active={activeScreen === "CryptoTools"}
                  label={t("nav.cryptoTools")}
                  onClick={() => onNavigate("CryptoTools")}
                />
                <NavButton
                  active={activeScreen === "OCR"}
                  label={t("nav.ocr")}
                  onClick={() => onNavigate("OCR")}
                />
                <NavButton
                  active={settingsActive}
                  label={t("nav.settings")}
                  onClick={() => onNavigate("Settings")}
                />
              </section>
            </div>
          </div>
        </aside>

        <section className="h-full min-h-0 min-w-0 overflow-y-auto bg-gradient-to-br from-white via-stone-50 to-slate-100 p-3 sm:p-4">
          {children}
        </section>
      </div>
    </main>
  );
}
