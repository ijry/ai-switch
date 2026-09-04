import { QueryClientProvider } from "@tanstack/react-query";
import { useCallback, useEffect, useRef, useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import { DeepLinkImportDialog } from "./components/deeplink/DeepLinkImportDialog";
import { LowDiskSpaceBanner } from "./components/system/LowDiskSpaceBanner";
import { AutoUpdatePrompt } from "./components/updates/AutoUpdatePrompt";
import {
  AppLayout,
  agentScreenByPlatform,
  platformByAgentScreen,
  type AgentPlatform,
} from "./components/layout/AppLayout";
import { WebAuthGate } from "./components/auth/WebAuthGate";
import { ErrorBoundary } from "./components/ui/ErrorBoundary";
import { I18nProvider } from "./lib/i18n";
import { createQueryClient } from "./lib/query/queryClient";
import { isDesktop, isLocalWebDevRuntime } from "./lib/transport";
import { AccountsScreen } from "./screens/AccountsScreen";
import { BatchesScreen } from "./screens/BatchesScreen";
import { DashboardScreen } from "./screens/DashboardScreen";
import { ImportsScreen } from "./screens/ImportsScreen";
import { OperationLogScreen } from "./screens/OperationLogScreen";
import { CryptoToolsScreen } from "./screens/CryptoToolsScreen";
import { OcrScreen } from "./screens/OcrScreen";
import { ProvidersScreen } from "./screens/ProvidersScreen";
import { SessionsScreen } from "./screens/SessionsScreen";
import { SettingsScreen } from "./screens/SettingsScreen";
import { TargetsScreen } from "./screens/TargetsScreen";
import { UpdatesScreen } from "./screens/UpdatesScreen";
import { VibeScreen } from "./screens/VibeScreen";
import { McpScreen } from "./screens/McpScreen";
import { SkillsScreen } from "./screens/SkillsScreen";
import { MotionPage, MotionProvider, type MotionDirection } from "./components/motion/MotionPrimitives";

const queryClient = createQueryClient();

const agentScreens = new Set([
  "Codex",
  "Claude",
  "Grok",
  "Gemini",
  "OpenCode",
  "OpenClaw",
  "Hermes",
]);

const implementedScreens = new Set([
  ...agentScreens,
  "Dashboard",
  "Batches",
  "Providers",
  "Imports",
  "Targets",
  "CryptoTools",
  "OCR",
  "Settings",
  "Sessions",
  "Updates",
  "Log",
  "MCP",
  "Skills",
  "Vibe",
]);

function canSkipWebAuthGate() {
  return isDesktop() || isLocalWebDevRuntime();
}

export type PoolScopeFocus = {
  platform: string;
  scope: "in_pool" | "out_of_pool";
  nonce: number;
};

export function App() {
  const [screen, setScreen] = useState("Codex");
  const screenRef = useRef("Codex");
  const [navigationDirection, setNavigationDirection] = useState<MotionDirection>("neutral");
  const [sessionPlatform, setSessionPlatform] = useState<string | null>(null);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [webReady, setWebReady] = useState(canSkipWebAuthGate);
  const [poolScopeFocus, setPoolScopeFocus] = useState<PoolScopeFocus | null>(null);
  // Vibe keeps live terminals; once it has been opened we keep it mounted and only
  // hide it so switching back and forth never drops running sessions or scrollback.
  const [vibeMounted, setVibeMounted] = useState(false);
  const agentPlatform = platformByAgentScreen[screen];
  const vibeActive = screen === "Vibe";

  useEffect(() => {
    setWebReady(canSkipWebAuthGate());
  }, []);

  useEffect(() => {
    if (vibeActive) {
      setVibeMounted(true);
    }
  }, [vibeActive]);

  const handleWebAuthenticated = useCallback(() => {
    queryClient.clear();
    setWebReady(true);
  }, []);

  const navigate = (nextScreen: string) => {
    const screens = Array.from(implementedScreens);
    const previousIndex = screens.indexOf(screenRef.current);
    const nextIndex = screens.indexOf(nextScreen);
    setNavigationDirection(
      previousIndex === -1 || nextIndex === -1
        ? "neutral"
        : nextIndex >= previousIndex
          ? "forward"
          : "backward",
    );
    screenRef.current = nextScreen;
    if (nextScreen === "Sessions") {
      setSessionPlatform(null);
    }
    setScreen(nextScreen);
  };

  const openSessions = (platform?: string | null) => {
    setNavigationDirection("forward");
    screenRef.current = "Sessions";
    setSessionPlatform(platform ?? null);
    setScreen("Sessions");
  };

  const handleDeepLinkImported = useCallback(
    (platform: string, options?: { joinedPool?: boolean }) => {
      const nextScreen = agentScreenByPlatform[platform as AgentPlatform];
      if (nextScreen) {
        setNavigationDirection("forward");
        screenRef.current = nextScreen;
        setScreen(nextScreen);
      }
      // Land the user on the segment where the imported account now lives so it
      // doesn't look like the import silently failed.
      setPoolScopeFocus({
        platform,
        scope: options?.joinedPool ? "in_pool" : "out_of_pool",
        nonce: Date.now(),
      });
    },
    [],
  );
  const handlePoolScopeFocusConsumed = useCallback((nonce: number) => {
    setPoolScopeFocus((current) => (current?.nonce === nonce ? null : current));
  }, []);

  return (
    <QueryClientProvider client={queryClient}>
      <I18nProvider>
        <MotionProvider>
        <DeepLinkImportDialog onImported={handleDeepLinkImported} />
        <AutoUpdatePrompt />
        {/* Gated on `webReady` so the poll never runs before the web token exists. */}
        {webReady && <LowDiskSpaceBanner />}
        {!webReady ? (
          <WebAuthGate onAuthenticated={handleWebAuthenticated} />
        ) : (
          <>
            {vibeMounted && (
              <motion.div
                aria-hidden={!vibeActive}
                className={vibeActive ? "vibe-host" : "vibe-host vibe-host--inactive"}
                animate={vibeActive ? { opacity: 1, scale: 1 } : { opacity: 0, scale: 0.985 }}
                initial={{ opacity: 0, scale: 0.985 }}
                transition={{ duration: 0.3, ease: [0.22, 1, 0.36, 1] }}
              >
                <ErrorBoundary label="Vibe">
                  <VibeScreen onExitVibe={() => navigate("Codex")} />
                </ErrorBoundary>
              </motion.div>
            )}
            {!vibeActive && (
              <AppLayout
                activeScreen={screen}
                onNavigate={navigate}
                onOpenVibe={() => navigate("Vibe")}
                onToggleSidebar={() => setSidebarCollapsed((value) => !value)}
                sidebarCollapsed={sidebarCollapsed}
              >
                <AnimatePresence initial={false} mode="wait">
                  <MotionPage direction={navigationDirection} key={screen}>
                    {agentPlatform && (
                  <AccountsScreen
                    onOpenSessions={openSessions}
                    platform={agentPlatform}
                    poolScopeFocus={poolScopeFocus}
                    onPoolScopeFocusConsumed={handlePoolScopeFocusConsumed}
                    sidebarCollapsed={sidebarCollapsed}
                  />
                )}
                {screen === "Dashboard" && <DashboardScreen />}
                {screen === "Batches" && <BatchesScreen />}
                {screen === "Providers" && <ProvidersScreen />}
                {screen === "Imports" && <ImportsScreen />}
                {screen === "Targets" && <TargetsScreen />}
                {screen === "CryptoTools" && <CryptoToolsScreen />}
                {screen === "OCR" && <OcrScreen />}
                {screen === "Sessions" && <SessionsScreen initialPlatform={sessionPlatform} />}
                {screen === "Updates" && <UpdatesScreen />}
                {screen === "Settings" && <SettingsScreen onOpenFeature={navigate} />}
                {screen === "MCP" && <McpScreen />}
                {screen === "Skills" && <SkillsScreen />}
                {screen === "Log" && <OperationLogScreen />}
                  {!implementedScreens.has(screen) && (
                    <div className="rounded-2xl border border-stone-200 bg-white/80 p-5 text-sm text-stone-500 shadow-sm">
                      {screen}
                    </div>
                  )}
                  </MotionPage>
                </AnimatePresence>
              </AppLayout>
            )}
          </>
        )}
        </MotionProvider>
      </I18nProvider>
    </QueryClientProvider>
  );
}
