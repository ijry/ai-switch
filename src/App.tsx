import { QueryClientProvider } from "@tanstack/react-query";
import { useCallback, useEffect, useState } from "react";
import { DeepLinkImportDialog } from "./components/deeplink/DeepLinkImportDialog";
import { AutoUpdatePrompt } from "./components/updates/AutoUpdatePrompt";
import {
  AppLayout,
  agentScreenByPlatform,
  platformByAgentScreen,
  type AgentPlatform,
} from "./components/layout/AppLayout";
import { WebAuthGate } from "./components/auth/WebAuthGate";
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
    if (nextScreen === "Sessions") {
      setSessionPlatform(null);
    }
    setScreen(nextScreen);
  };

  const openSessions = (platform?: string | null) => {
    setSessionPlatform(platform ?? null);
    setScreen("Sessions");
  };

  const handleDeepLinkImported = useCallback(
    (platform: string, options?: { joinedPool?: boolean }) => {
      const nextScreen = agentScreenByPlatform[platform as AgentPlatform];
      if (nextScreen) {
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
        <DeepLinkImportDialog onImported={handleDeepLinkImported} />
        <AutoUpdatePrompt />
        {!webReady ? (
          <WebAuthGate onAuthenticated={handleWebAuthenticated} />
        ) : (
          <>
            {vibeMounted && (
              <div aria-hidden={!vibeActive} className={vibeActive ? "contents" : "hidden"}>
                <VibeScreen onExitVibe={() => setScreen("Codex")} />
              </div>
            )}
            {!vibeActive && (
              <AppLayout
                activeScreen={screen}
                onNavigate={navigate}
                onOpenVibe={() => setScreen("Vibe")}
                onToggleSidebar={() => setSidebarCollapsed((value) => !value)}
                sidebarCollapsed={sidebarCollapsed}
              >
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
              </AppLayout>
            )}
          </>
        )}
      </I18nProvider>
    </QueryClientProvider>
  );
}
