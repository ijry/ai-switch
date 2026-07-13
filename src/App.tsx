import { QueryClientProvider } from "@tanstack/react-query";
import { useState } from "react";
import { AppLayout } from "./components/layout/AppLayout";
import { createQueryClient } from "./lib/query/queryClient";
import { AccountsScreen } from "./screens/AccountsScreen";
import { BatchesScreen } from "./screens/BatchesScreen";
import { BulkScreen } from "./screens/BulkScreen";
import { DashboardScreen } from "./screens/DashboardScreen";
import { ImportsScreen } from "./screens/ImportsScreen";
import { InstancesScreen } from "./screens/InstancesScreen";
import { LibraryScreen } from "./screens/LibraryScreen";
import { McpScreen } from "./screens/McpScreen";
import { OperationLogScreen } from "./screens/OperationLogScreen";
import { ProvidersScreen } from "./screens/ProvidersScreen";
import { RoutingScreen } from "./screens/RoutingScreen";
import { SessionsScreen } from "./screens/SessionsScreen";
import { SettingsScreen } from "./screens/SettingsScreen";
import { SyncScreen } from "./screens/SyncScreen";
import { TargetsScreen } from "./screens/TargetsScreen";
import { UpdatesScreen } from "./screens/UpdatesScreen";
import { WakeupsScreen } from "./screens/WakeupsScreen";

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
        {screen === "Instances" && <InstancesScreen />}
        {screen === "Wakeups" && <WakeupsScreen />}
        {screen === "Bulk" && <BulkScreen />}
        {screen === "Targets" && <TargetsScreen />}
        {screen === "MCP" && <McpScreen />}
        {screen === "Library" && <LibraryScreen />}
        {screen === "Routing" && <RoutingScreen />}
        {screen === "Sync" && <SyncScreen />}
        {screen === "Sessions" && <SessionsScreen />}
        {screen === "Updates" && <UpdatesScreen />}
        {screen === "Settings" && <SettingsScreen />}
        {screen === "Log" && <OperationLogScreen />}
        {![
          "Dashboard",
          "Batches",
          "Providers",
          "Accounts",
          "Imports",
          "Instances",
          "Wakeups",
          "Bulk",
          "Targets",
          "MCP",
          "Library",
          "Routing",
          "Sync",
          "Sessions",
          "Updates",
          "Settings",
          "Log",
        ].includes(
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
