import { useEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import {
  Menu,
  PlugZap,
  Settings2,
  Sparkles,
  TerminalSquare,
  type LucideIcon,
} from "lucide-react";
import { AiSwitchLogo } from "../brand/AiSwitchLogo";
import { AgentIcon, type AgentIconPlatform } from "../brand/AgentIcon";
import { supportedLanguages, useI18n, type Language } from "../../lib/i18n";
import { useDragResize } from "../../lib/useDragResize";

export type AgentPlatform =
  | "codex"
  | "claude"
  | "grok"
  | "gemini"
  | "opencode"
  | "openclaw"
  | "hermes";

export const agentPlatforms: AgentPlatform[] = [
  "codex",
  "claude",
  "grok",
  "gemini",
  "opencode",
  "openclaw",
  "hermes",
];

export const agentScreenByPlatform: Record<AgentPlatform, string> = {
  codex: "Codex",
  claude: "Claude",
  grok: "Grok",
  gemini: "Gemini",
  opencode: "OpenCode",
  openclaw: "OpenClaw",
  hermes: "Hermes",
};

export const platformByAgentScreen: Record<string, AgentPlatform> = {
  Codex: "codex",
  Claude: "claude",
  Grok: "grok",
  Gemini: "gemini",
  OpenCode: "opencode",
  OpenClaw: "openclaw",
  Hermes: "hermes",
};

export const settingsFeatureScreens = [
  "Sessions",
  "Updates",
  "Log",
] as const;

const SIDEBAR_DEFAULT_WIDTH = 216;
const SIDEBAR_MIN_WIDTH = 180;
const SIDEBAR_MAX_WIDTH = 320;
const SIDEBAR_WIDTH_STORAGE_KEY = "ai-switch.sidebar-width";
const SIDEBAR_DRAWER_BREAKPOINT = 600;

function clampSidebarWidth(value: number) {
  return Math.min(Math.max(value, SIDEBAR_MIN_WIDTH), SIDEBAR_MAX_WIDTH);
}

function readSidebarWidth() {
  if (typeof window === "undefined") {
    return SIDEBAR_DEFAULT_WIDTH;
  }

  try {
    const rawValue = window.localStorage.getItem(SIDEBAR_WIDTH_STORAGE_KEY);
    if (!rawValue) {
      return SIDEBAR_DEFAULT_WIDTH;
    }
    const storedValue = Number(rawValue);
    return Number.isFinite(storedValue) ? clampSidebarWidth(storedValue) : SIDEBAR_DEFAULT_WIDTH;
  } catch {
    return SIDEBAR_DEFAULT_WIDTH;
  }
}

type AppLayoutProps = {
  children: ReactNode;
  activeScreen: string;
  onNavigate: (screen: string) => void;
  onOpenVibe?: () => void;
  onToggleSidebar: () => void;
  onLanguageChange?: (language: Language) => void;
  languageSaving?: boolean;
  sidebarCollapsed: boolean;
};

type AgentNavItem = {
  icon: AgentIconPlatform;
  screen: string;
  platform: AgentPlatform;
  labelKey:
    | "nav.agent.codex"
    | "nav.agent.claude"
    | "nav.agent.grok"
    | "nav.agent.gemini"
    | "nav.agent.opencode"
    | "nav.agent.openclaw"
    | "nav.agent.hermes";
};

const agentItems: AgentNavItem[] = [
  { icon: "codex", screen: "Codex", platform: "codex", labelKey: "nav.agent.codex" },
  { icon: "claude", screen: "Claude", platform: "claude", labelKey: "nav.agent.claude" },
  { icon: "grok", screen: "Grok", platform: "grok", labelKey: "nav.agent.grok" },
  { icon: "gemini", screen: "Gemini", platform: "gemini", labelKey: "nav.agent.gemini" },
  { icon: "opencode", screen: "OpenCode", platform: "opencode", labelKey: "nav.agent.opencode" },
  { icon: "openclaw", screen: "OpenClaw", platform: "openclaw", labelKey: "nav.agent.openclaw" },
  { icon: "hermes", screen: "Hermes", platform: "hermes", labelKey: "nav.agent.hermes" },
];

function isSettingsArea(screen: string) {
  return screen === "Settings" || (settingsFeatureScreens as readonly string[]).includes(screen);
}

function NavButton({
  collapsed,
  icon,
  label,
  active,
  onClick,
  variant = "standard",
}: {
  collapsed: boolean;
  icon: LucideIcon | AgentIconPlatform;
  label: string;
  active: boolean;
  onClick: () => void;
  variant?: "primary" | "standard";
}) {
  const baseClasses = `group flex w-full items-center rounded-xl border py-2 text-left text-[13px] transition-colors duration-150 focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-400 ${
    collapsed ? "justify-center px-0" : "justify-between px-3"
  }`;
  const activeClasses =
    variant === "primary"
      ? "border-stone-300 bg-white text-stone-950 shadow-sm"
      : "border-stone-300 bg-stone-100 text-stone-950 shadow-sm";
  const idleClasses =
    variant === "primary"
      ? "border-transparent bg-transparent text-stone-600 hover:bg-white/60 hover:text-stone-950"
      : "border-transparent bg-transparent text-stone-600 hover:bg-stone-100 hover:text-stone-950";
  const LucideIconComponent = typeof icon === "string" ? null : icon;

  return (
    <button
      aria-current={active ? "page" : undefined}
      className={`${baseClasses} ${active ? activeClasses : idleClasses}`}
      onClick={onClick}
      title={label}
      type="button"
    >
      <span
        className={`flex min-w-0 items-center ${collapsed ? "justify-center gap-0" : "gap-2"}`}
      >
        <span
          className={`h-1.5 w-1.5 shrink-0 rounded-full ${collapsed ? "hidden" : ""} ${
            active ? "bg-amber-500" : "bg-stone-300 group-hover:bg-stone-400"
          }`}
        />
        {typeof icon === "string" ? (
          <AgentIcon className="h-4 w-4" platform={icon} />
        ) : (
          LucideIconComponent ? (
            <LucideIconComponent aria-hidden="true" className={`h-4 w-4 shrink-0 ${active ? "text-amber-600" : "text-stone-500"}`} />
          ) : null
        )}
        <span className={`truncate font-medium ${collapsed ? "sr-only" : ""}`}>{label}</span>
      </span>
      <span
        aria-hidden="true"
        className={`${collapsed ? "hidden" : ""} ${
          active ? "text-stone-400" : "text-transparent"
        }`}
      >
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
  onToggleSidebar,
  onLanguageChange,
  languageSaving = false,
  sidebarCollapsed,
}: AppLayoutProps) {
  const { language, setLanguage, t } = useI18n();
  const appShellRef = useRef<HTMLDivElement | null>(null);
  const [sidebarWidth, setSidebarWidth] = useState(readSidebarWidth);
  const [narrowLayout, setNarrowLayout] = useState(
    () => typeof window !== "undefined" && window.innerWidth < SIDEBAR_DRAWER_BREAKPOINT,
  );
  const [sidebarDrawerOpen, setSidebarDrawerOpen] = useState(false);
  const settingsActive = isSettingsArea(activeScreen);
  const accountWorkspaceActive = agentItems.some((item) => item.screen === activeScreen);
  const sidebarDrawerVisible = narrowLayout && sidebarDrawerOpen;
  const sidebarContentCollapsed = narrowLayout ? !sidebarDrawerOpen : sidebarCollapsed;
  const desktopGridClass = sidebarCollapsed
    ? "min-[600px]:grid-cols-[56px_minmax(0,1fr)]"
    : "min-[600px]:grid-cols-[var(--app-sidebar-width)_minmax(0,1fr)]";
  const { dragging: sidebarResizing, startDragging: startSidebarResize } = useDragResize({
    axis: "x",
    min: SIDEBAR_MIN_WIDTH,
    max: SIDEBAR_MAX_WIDTH,
    getInitialValue: () => sidebarWidth,
    getValueFromPointer: (event) => {
      const shellRect = appShellRef.current?.getBoundingClientRect();
      return shellRect ? event.clientX - shellRect.left : sidebarWidth;
    },
    onChange: setSidebarWidth,
  });

  useEffect(() => {
    try {
      window.localStorage.setItem(SIDEBAR_WIDTH_STORAGE_KEY, String(clampSidebarWidth(sidebarWidth)));
    } catch {
      // Storage may be unavailable in restricted webviews.
    }
  }, [sidebarWidth]);

  useEffect(() => {
    const syncNarrowLayout = () => {
      const nextNarrowLayout = window.innerWidth < SIDEBAR_DRAWER_BREAKPOINT;
      setNarrowLayout(nextNarrowLayout);
      if (!nextNarrowLayout) {
        setSidebarDrawerOpen(false);
      }
    };
    syncNarrowLayout();
    window.addEventListener("resize", syncNarrowLayout);
    return () => window.removeEventListener("resize", syncNarrowLayout);
  }, []);

  useEffect(() => {
    if (!sidebarDrawerVisible) {
      return;
    }
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setSidebarDrawerOpen(false);
      }
    };
    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [sidebarDrawerVisible]);

  const handleLanguageChange = (nextLanguage: Language) => {
    if (onLanguageChange) {
      onLanguageChange(nextLanguage);
      return;
    }

    setLanguage(nextLanguage);
  };

  const handleToggleSidebar = () => {
    if (narrowLayout) {
      setSidebarDrawerOpen((current) => !current);
      return;
    }
    onToggleSidebar();
  };

  const handleNavigate = (nextScreen: string) => {
    setSidebarDrawerOpen(false);
    onNavigate(nextScreen);
  };

  const handleOpenVibe = () => {
    setSidebarDrawerOpen(false);
    onOpenVibe?.();
  };

  return (
    <main className="box-border h-screen max-h-[100dvh] overflow-hidden text-stone-950">
      <div
        className={`box-border grid h-full min-h-0 grid-cols-[56px_minmax(0,1fr)] ${desktopGridClass}`}
        data-testid="app-shell"
        ref={appShellRef}
        style={
          {
            "--app-sidebar-width": `${sidebarContentCollapsed ? 56 : sidebarWidth}px`,
          } as CSSProperties
        }
      >
        {sidebarDrawerVisible && (
          <div
            aria-hidden="true"
            className="app-sidebar-drawer-backdrop"
            data-testid="app-sidebar-drawer-backdrop"
            onClick={() => setSidebarDrawerOpen(false)}
          />
        )}
        <aside
          className={`${
            sidebarDrawerVisible ? "app-sidebar-drawer" : "relative"
          } flex h-full min-h-0 flex-col overflow-hidden border-r border-white/80 bg-gradient-to-br from-slate-50/92 via-emerald-50/74 to-amber-50/70 shadow-xl shadow-stone-900/5 backdrop-blur-2xl ${
            sidebarContentCollapsed ? "p-2" : "p-3"
          }`}
          data-testid="app-sidebar"
        >
          <div className="pointer-events-none absolute inset-0 bg-[radial-gradient(circle_at_20%_15%,rgba(16,185,129,0.18),transparent_34%),radial-gradient(circle_at_88%_8%,rgba(245,158,11,0.16),transparent_30%),linear-gradient(180deg,rgba(255,255,255,0.72),rgba(255,255,255,0.38))]" />
          <div className="relative flex min-h-0 flex-1 flex-col">
            <div
              className={`flex justify-between rounded-2xl border border-white/80 bg-white/56 shadow-sm backdrop-blur-xl ${
                sidebarContentCollapsed
                  ? "mb-4 flex-col items-center gap-2 p-1"
                  : "mb-5 items-start gap-3 p-3"
              }`}
            >
              <div
                className={`min-w-0 items-center gap-2 ${
                  sidebarContentCollapsed ? "hidden" : "flex"
                }`}
              >
                <AiSwitchLogo className="h-9 w-9 shrink-0 rounded-2xl shadow-sm" />
                <div className="min-w-0">
                  <p className="truncate text-[13px] font-semibold text-stone-950">AI Switch</p>
                  <p className="truncate text-[11px] text-stone-500">{t("layout.brandBadge")}</p>
                </div>
              </div>
              <div
                className={`flex items-center gap-2 ${
                  sidebarContentCollapsed ? "flex-col" : ""
                }`}
              >
                <button
                  aria-expanded={!sidebarContentCollapsed}
                  aria-label={
                    sidebarContentCollapsed
                      ? t("layout.expandSidebar")
                      : t("layout.collapseSidebar")
                  }
                  className="grid h-8 w-8 shrink-0 place-items-center rounded-xl border border-stone-200 bg-white/70 text-stone-600 shadow-sm transition-colors hover:border-stone-300 hover:bg-white hover:text-stone-950 focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-400"
                  onClick={handleToggleSidebar}
                  title={
                    sidebarContentCollapsed
                      ? t("layout.expandSidebar")
                      : t("layout.collapseSidebar")
                  }
                  type="button"
                >
                  <Menu aria-hidden="true" className="h-4 w-4" />
                </button>
                <button
                  aria-label={t("layout.switchToVibe")}
                  className="grid h-8 w-8 shrink-0 place-items-center rounded-xl border border-stone-200 bg-white/70 text-stone-600 shadow-sm transition-colors hover:border-stone-300 hover:bg-white hover:text-stone-950 focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-400"
                  onClick={handleOpenVibe}
                  title={t("layout.switchToVibe")}
                  type="button"
                >
                  <TerminalSquare aria-hidden="true" className="h-4 w-4" />
                </button>
              </div>
            </div>

            <label
              className={`mb-5 items-center justify-between gap-2 rounded-2xl border border-white/70 bg-white/50 px-3 py-2 text-[12px] font-medium text-stone-500 backdrop-blur-xl ${
                sidebarContentCollapsed ? "hidden" : "flex"
              }`}
            >
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
                <p
                  className={`px-2 pb-1 text-[11px] font-semibold uppercase tracking-wide text-stone-400 ${
                    sidebarContentCollapsed ? "hidden" : ""
                  }`}
                >
                  {t("layout.agents")}
                </p>
                <div className="space-y-1">
                  {agentItems.map((item) => (
                    <NavButton
                      active={activeScreen === item.screen}
                      collapsed={sidebarContentCollapsed}
                      icon={item.icon}
                      key={item.screen}
                      label={t(item.labelKey)}
                      onClick={() => handleNavigate(item.screen)}
                      variant="primary"
                    />
                  ))}
                </div>
              </section>

              <section>
                <p
                  className={`px-2 pb-1 text-[11px] font-semibold uppercase tracking-wide text-stone-400 ${
                    sidebarContentCollapsed ? "hidden" : ""
                  }`}
                >
                  {t("layout.system")}
                </p>
                <NavButton
                  active={activeScreen === "MCP"}
                  collapsed={sidebarContentCollapsed}
                  icon={PlugZap}
                  label={t("nav.mcp")}
                  onClick={() => handleNavigate("MCP")}
                />
                <NavButton
                  active={activeScreen === "Skills"}
                  collapsed={sidebarContentCollapsed}
                  icon={Sparkles}
                  label={t("nav.skills")}
                  onClick={() => handleNavigate("Skills")}
                />
                <NavButton
                  active={settingsActive}
                  collapsed={sidebarContentCollapsed}
                  icon={Settings2}
                  label={t("nav.settings")}
                  onClick={() => handleNavigate("Settings")}
                />
              </section>
            </div>
          </div>
          {!narrowLayout && !sidebarCollapsed && (
            <div
              aria-label={t("layout.resizeSidebar")}
              aria-orientation="vertical"
              aria-valuemax={SIDEBAR_MAX_WIDTH}
              aria-valuemin={SIDEBAR_MIN_WIDTH}
              aria-valuenow={sidebarWidth}
              className={`absolute inset-y-0 right-0 z-20 w-1.5 touch-none select-none cursor-col-resize bg-transparent transition-colors hover:bg-stone-300/70 ${
                sidebarResizing ? "bg-blue-400/70" : ""
              }`}
              data-testid="sidebar-resize-handle"
              onPointerDown={startSidebarResize}
              role="separator"
              title={t("layout.resizeSidebar")}
            />
          )}
        </aside>

        <section
          className={`col-start-2 box-border h-full min-h-0 min-w-0 bg-stone-100 ${
            accountWorkspaceActive ? "overflow-hidden p-0" : "overflow-y-auto p-2 sm:p-3"
          }`}
        >
          {children}
        </section>
      </div>
    </main>
  );
}
