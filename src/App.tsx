import { QueryClientProvider } from "@tanstack/react-query";
import { useCallback, useEffect, useState } from "react";
import { DeepLinkImportDialog } from "./components/deeplink/DeepLinkImportDialog";
import {
  AppLayout,
  agentScreenByPlatform,
  platformByAgentScreen,
  type AgentPlatform,
} from "./components/layout/AppLayout";
import { WebAuthGate } from "./components/auth/WebAuthGate";
import { I18nProvider } from "./lib/i18n";
import { createQueryClient } from "./lib/query/queryClient";
import { isDesktop } from "./lib/transport";
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
  "Vibe",
]);

export function App() {
  const [screen, setScreen] = useState("Codex");
  const [sessionPlatform, setSessionPlatform] = useState<string | null>(null);
  const [webReady, setWebReady] = useState(() => isDesktop());
  const agentPlatform = platformByAgentScreen[screen];

  useEffect(() => {
    setWebReady(isDesktop());
  }, []);

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

  const handleDeepLinkImported = useCallback((platform: string) => {
    const nextScreen = agentScreenByPlatform[platform as AgentPlatform];
    if (nextScreen) {
      setScreen(nextScreen);
    }
  }, []);

  return (
    <QueryClientProvider client={queryClient}>
      <I18nProvider>
        <DeepLinkImportDialog onImported={handleDeepLinkImported} />
        {!webReady ? (
          <WebAuthGate onAuthenticated={handleWebAuthenticated} />
        ) : screen === "Vibe" ? (
          <VibeScreen onExitVibe={() => setScreen("Codex")} />
        ) : (
          <AppLayout activeScreen={screen} onNavigate={navigate} onOpenVibe={() => setScreen("Vibe")}>
            {agentPlatform && <AccountsScreen onOpenSessions={openSessions} platform={agentPlatform} />}
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
            {screen === "Log" && <OperationLogScreen />}
            {!implementedScreens.has(screen) && (
              <div className="rounded-2xl border border-stone-200 bg-white/80 p-5 text-sm text-stone-500 shadow-sm">
                {screen}
              </div>
            )}
          </AppLayout>
        )}
      </I18nProvider>
    </QueryClientProvider>
  );
}
