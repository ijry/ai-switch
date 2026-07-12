import { QueryClientProvider } from "@tanstack/react-query";
import { useState } from "react";
import { AppLayout } from "./components/layout/AppLayout";
import { createQueryClient } from "./lib/query/queryClient";
import { AccountsScreen } from "./screens/AccountsScreen";
import { BatchesScreen } from "./screens/BatchesScreen";
import { DashboardScreen } from "./screens/DashboardScreen";
import { ImportsScreen } from "./screens/ImportsScreen";
import { OperationLogScreen } from "./screens/OperationLogScreen";
import { ProvidersScreen } from "./screens/ProvidersScreen";
import { SettingsScreen } from "./screens/SettingsScreen";
import { TargetsScreen } from "./screens/TargetsScreen";

const queryClient = createQueryClient();

export function App() {
  const [screen, setScreen] = useState("Dashboard");

  return (
    <QueryClientProvider client={queryClient}>
      <AppLayout activeScreen={screen} onNavigate={setScreen}>
        {screen === "Dashboard" && <DashboardScreen />}
        {screen === "Batches" && <BatchesScreen />}
        {screen === "Providers" && <ProvidersScreen />}
        {screen === "Accounts" && <AccountsScreen />}
        {screen === "Imports" && <ImportsScreen />}
        {screen === "Targets" && <TargetsScreen />}
        {screen === "Settings" && <SettingsScreen />}
        {screen === "Log" && <OperationLogScreen />}
        {!["Dashboard", "Batches", "Providers", "Accounts", "Imports", "Targets", "Settings", "Log"].includes(
          screen,
        ) && (
          <div className="rounded-3xl border border-ink/10 bg-white/75 p-6 text-steel shadow-sm">
            {screen} foundation screen.
          </div>
        )}
      </AppLayout>
    </QueryClientProvider>
  );
}
