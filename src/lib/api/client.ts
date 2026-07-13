import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  Batch,
  BatchGroup,
  ImportJob,
  CreateApiRouteCredentialInput,
  CreateTerminalSessionInput,
  NewOfficialAccount,
  OfficialAccount,
  RouteConfigWriteOutcome,
  RouteCredential,
  RouteCredentialImportResult,
  RoutePoolRouteOutcome,
  RoutePoolRouteRequest,
  RoutePoolState,
  RouteProxyStatus,
  SessionMessage,
  SessionMeta,
  TargetApp,
  TerminalSession,
  UpdateOfficialAccount,
  UpdateRouteCredentialInput,
} from "./types";

export function listBatchGroups(search?: string): Promise<BatchGroup[]> {
  return invoke("list_batch_groups", { search: search || null });
}

export function createBatch(input: {
  name: string;
  source: string;
  notes?: string | null;
}): Promise<Batch> {
  return invoke("create_batch", { input });
}

export function createOfficialAccount(request: {
  account: NewOfficialAccount;
  batch_id?: string | null;
}): Promise<OfficialAccount> {
  return invoke("create_official_account", { request });
}

export function getOfficialAccount(id: string): Promise<OfficialAccount> {
  return invoke("get_official_account", { id });
}

export function updateOfficialAccount(input: {
  id: string;
  account: UpdateOfficialAccount;
}): Promise<OfficialAccount> {
  return invoke("update_official_account", { input });
}

export function importExampleJson(request: {
  batch_name: string;
  source_label: string;
  strategy: string;
  json: string;
}): Promise<ImportJob> {
  return invoke("import_example_json", { request });
}

export function listTargetApps(): Promise<TargetApp[]> {
  return invoke("list_target_apps");
}

export function getRoutePool(platform: string): Promise<RoutePoolState> {
  return invoke("get_route_pool", { platform });
}

export function setRoutePoolMembers(input: {
  platform: string;
  account_ids: string[];
}): Promise<RoutePoolState> {
  return invoke("set_route_pool_members", { input });
}

export function routePoolRouteOnce(request: RoutePoolRouteRequest): Promise<RoutePoolRouteOutcome> {
  return invoke("route_pool_route_once", { request });
}

export function getSettings(): Promise<AppSettings> {
  return invoke("get_settings");
}

export function saveSettings(settings: AppSettings): Promise<AppSettings> {
  return invoke("save_settings", { settings });
}

export function startRouteProxy(): Promise<RouteProxyStatus> {
  return invoke("start_route_proxy");
}

export function stopRouteProxy(): Promise<RouteProxyStatus> {
  return invoke("stop_route_proxy");
}

export function getRouteProxyStatus(): Promise<RouteProxyStatus> {
  return invoke("get_route_proxy_status");
}

export function writeRouteProxyConfigs(baseUrl?: string | null): Promise<RouteConfigWriteOutcome[]> {
  return invoke("write_route_proxy_configs", { baseUrl: baseUrl ?? null });
}

export function listRouteCredentials(platform: string): Promise<RouteCredential[]> {
  return invoke("list_route_credentials", { platform });
}

export function createApiRouteCredential(input: CreateApiRouteCredentialInput): Promise<RouteCredential> {
  return invoke("create_api_route_credential", { input });
}

export function importOfficialRouteCredentialsFromText(input: {
  platform: string;
  text: string;
  batch_name?: string | null;
}): Promise<RouteCredentialImportResult> {
  return invoke("import_official_route_credentials_from_text", { input });
}

export function importOfficialRouteCredentialsFromFiles(input: {
  platform: string;
  file_paths: string[];
  batch_name?: string | null;
}): Promise<RouteCredentialImportResult> {
  return invoke("import_official_route_credentials_from_files", { input });
}

export function updateRouteCredential(
  id: string,
  input: UpdateRouteCredentialInput,
): Promise<RouteCredential> {
  return invoke("update_route_credential", { id, input });
}

export function deleteRouteCredential(id: string): Promise<void> {
  return invoke("delete_route_credential", { id });
}

export function listSessions(platform?: string | null): Promise<SessionMeta[]> {
  return invoke("list_sessions", { platform: platform ?? null });
}

export function getSessionMessages(input: {
  providerId: string;
  sourcePath: string;
}): Promise<SessionMessage[]> {
  return invoke("get_session_messages", {
    providerId: input.providerId,
    sourcePath: input.sourcePath,
  });
}

export function createTerminalSession(
  input: CreateTerminalSessionInput,
): Promise<TerminalSession> {
  return invoke("create_terminal_session", { input });
}

export function writeTerminalInput(sessionId: string, data: string): Promise<void> {
  return invoke("write_terminal_input", { sessionId, data });
}

export function resizeTerminal(
  sessionId: string,
  cols: number,
  rows: number,
): Promise<void> {
  return invoke("resize_terminal", { sessionId, cols, rows });
}

export function killTerminalSession(sessionId: string): Promise<void> {
  return invoke("kill_terminal_session", { sessionId });
}

export function listTerminalSessions(): Promise<TerminalSession[]> {
  return invoke("list_terminal_sessions");
}
