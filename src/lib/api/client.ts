import { getTransport } from "../transport";
import type {
  AccountStatus,
  AgentLaunchOption,
  AppSettings,
  AppSettingsView,
  Batch,
  BatchGroup,
  ConfigSnapshotSummary,
  ConfigWriteClientStatus,
  ConfigWriteOutcome,
  CopyRouteCredentialInput,
  ExternalClientImportOutcome,
  ExternalClientImportPreview,
  FetchedRouteModel,
  ImportExternalClientAccountsInput,
  ImportJob,
  CreateApiRouteCredentialInput,
  CreateTerminalSessionInput,
  NewOfficialAccount,
  OfficialAccount,
  PlatformCapability,
  ExportRouteCredentialsInput,
  RouteCredentialExportResult,
  ImportRouteCredentialsInput,
  PreviewExternalClientImportInput,
  PreviewRouteCredentialImportInput,
  RouteCredentialImportOutcome,
  RouteCredentialImportPreview,
  RouteCredential,
  RouteCredentialPage,
  RouteCredentialPageRequest,
  ReorderRouteCredentialInput,
  RouteCredentialImportResult,
  QuotaRefreshOutcome,
  RecoveryRule,
  RouteModelsFetchRequest,
  RoutePoolModelTestOutcome,
  RoutePoolModelTestRequest,
  RoutePoolRouteOutcome,
  RoutePoolRouteRequest,
  RoutePoolState,
  RouteProxyLiveLogEntry,
  RouteProxyHttpsOperationOutcome,
  RouteProxyHttpsStatus,
  RouteProxyStatus,
  SaveRouteCredentialExportResult,
  TailscaleLogin,
  TailscaleStatus,
  SessionMessage,
  SessionMeta,
  SessionUsageStats,
  TargetApp,
  TargetConfigStatus,
  TerminalSession,
  WebServerStatus,
  WebServiceConfig,
  UpdateOfficialAccount,
  UpdateRouteCredentialInput,
  LocalMcpServer,
  MobilePairingPayload,
  McpAppType,
  McpMarketplaceItem,
  McpMarketplaceProvider,
  McpMarketplaceServerDetail,
  McpSpec,
  SkillAgentInfo,
  SkillAgentType,
  SkillContent,
  SkillItem,
  SkillLayout,
  SkillPackageDetail,
  SkillPackageInstallResult,
  SkillScope,
  SkillsListResult,
  SkillsPackageListResult,
} from "./types";

function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return getTransport().call<T>(command, args);
}

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

export function listTargetConfigStatuses(): Promise<TargetConfigStatus[]> {
  return invoke("list_target_config_statuses");
}

export function listConfigSnapshots(
  targetAppId?: string | null,
  limit?: number | null,
): Promise<ConfigSnapshotSummary[]> {
  return invoke("list_config_snapshots", {
    targetAppId: targetAppId ?? null,
    limit: limit ?? null,
  });
}

export function rollbackConfigSnapshot(id: string): Promise<ConfigWriteOutcome> {
  return invoke("rollback_config_snapshot", { id });
}

export function listPlatformCapabilities(): Promise<PlatformCapability[]> {
  return invoke("list_platform_capabilities");
}

export function getRoutePool(
  platform: string,
  since?: string | null,
  requestPage?: number | null,
  requestPageSize?: number | null,
): Promise<RoutePoolState> {
  return invoke("get_route_pool", {
    platform,
    since: since ?? null,
    request_page: requestPage ?? null,
    request_page_size: requestPageSize ?? null,
  });
}

export function setRoutePoolMembers(input: {
  platform: string;
  account_ids: string[];
}): Promise<RoutePoolState> {
  return invoke("set_route_pool_members", { input });
}

/**
 * Aggregate usage from local Claude Code / Codex CLI session transcripts.
 *
 * `since` is an RFC 3339 timestamp; pass null for the full history.
 */
export function getSessionUsageStats(since?: string | null): Promise<SessionUsageStats> {
  return invoke("get_session_usage_stats", { since: since ?? null });
}

/** Reload `~/.ai-switch/model-prices.json`, returning the entry count. */
export function reloadModelPriceOverrides(): Promise<number> {
  return invoke("reload_model_price_overrides");
}

export function routePoolRouteOnce(request: RoutePoolRouteRequest): Promise<RoutePoolRouteOutcome> {
  return invoke("route_pool_route_once", { request });
}

export function routePoolTestModel(request: RoutePoolModelTestRequest): Promise<RoutePoolModelTestOutcome> {
  return invoke("route_pool_test_model", { request });
}

export function fetchRouteModels(request: RouteModelsFetchRequest): Promise<FetchedRouteModel[]> {
  return invoke("fetch_route_models", { request });
}

export function subscribeRouteProxyLiveLog(platform: string): Promise<RouteProxyLiveLogEntry[]> {
  return invoke("subscribe_route_proxy_live_log", { platform });
}

export function unsubscribeRouteProxyLiveLog(): Promise<void> {
  return invoke("unsubscribe_route_proxy_live_log");
}

export function getSettings(): Promise<AppSettingsView> {
  return invoke("get_settings");
}

export function saveSettings(settings: AppSettings): Promise<AppSettingsView> {
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

export function getRouteProxyKey(platform: string): Promise<string> {
  return invoke("get_route_proxy_key", { platform });
}

export function getRouteProxyHttpsStatus(): Promise<RouteProxyHttpsStatus> {
  return invoke("get_route_proxy_https_status");
}

export function enableRouteProxyHttps(): Promise<RouteProxyHttpsOperationOutcome> {
  return invoke("enable_route_proxy_https");
}

export function disableRouteProxyHttps(): Promise<RouteProxyHttpsOperationOutcome> {
  return invoke("disable_route_proxy_https");
}

export function reimportRouteProxyRootCa(): Promise<RouteProxyHttpsOperationOutcome> {
  return invoke("reimport_route_proxy_root_ca");
}

export function regenerateRouteProxyHttpsCertificates(): Promise<RouteProxyHttpsOperationOutcome> {
  return invoke("regenerate_route_proxy_https_certificates");
}

export function uninstallRouteProxyRootCa(): Promise<RouteProxyHttpsOperationOutcome> {
  return invoke("uninstall_route_proxy_root_ca");
}

export function deleteRouteProxyHttpsCertificates(): Promise<RouteProxyHttpsStatus> {
  return invoke("delete_route_proxy_https_certificates");
}

export function openRouteProxyHttpsCertificateDirectory(): Promise<void> {
  return invoke("open_route_proxy_https_certificate_dir");
}

export function writeRouteProxyConfigs(
  baseUrl: string | null | undefined,
  platform: string,
  clientKeys?: string[] | null,
): Promise<ConfigWriteOutcome[]> {
  return invoke("write_route_proxy_configs", {
    baseUrl: baseUrl ?? null,
    platform,
    clientKeys: clientKeys ?? null,
  });
}

/// Whether writing config now would change the file on disk. Config is written
/// on demand, so model-slot and global-client-config edits sit unapplied until
/// the user asks for a write.
export function routeConfigWriteIsStale(
  baseUrl: string | null | undefined,
  platform: string,
  clientKeys?: string[] | null,
): Promise<boolean> {
  return invoke("route_config_write_is_stale", {
    baseUrl: baseUrl ?? null,
    platform,
    clientKeys: clientKeys ?? null,
  });
}

/// Clients this platform can write config for, with each one's current file
/// state. Drives the write dialog's checkbox list.
export function listConfigWriteClients(platform: string): Promise<ConfigWriteClientStatus[]> {
  return invoke("list_config_write_clients", { platform });
}

export function listRouteCredentials(platform: string): Promise<RouteCredential[]> {
  return invoke("list_route_credentials", { platform });
}

export function exportRouteCredentials(
  input: ExportRouteCredentialsInput,
): Promise<RouteCredentialExportResult> {
  return invoke("export_route_credentials", { input });
}

export function saveRouteCredentialExport(input: {
  suggested_file_name: string;
  json_text: string;
}): Promise<SaveRouteCredentialExportResult> {
  return invoke("save_route_credential_export", {
    suggested_file_name: input.suggested_file_name,
    json_text: input.json_text,
  });
}

export function previewRouteCredentialImport(
  input: PreviewRouteCredentialImportInput,
): Promise<RouteCredentialImportPreview> {
  return invoke("preview_route_credential_import", { input });
}

export function importRouteCredentials(
  input: ImportRouteCredentialsInput,
): Promise<RouteCredentialImportOutcome> {
  return invoke("import_route_credentials", { input });
}

export function previewExternalClientImport(
  input: PreviewExternalClientImportInput,
): Promise<ExternalClientImportPreview> {
  return invoke("preview_external_client_import", { input });
}

export function importExternalClientAccounts(
  input: ImportExternalClientAccountsInput,
): Promise<ExternalClientImportOutcome> {
  return invoke("import_external_client_accounts", { input });
}

export function listRouteCredentialPage(input: RouteCredentialPageRequest): Promise<RouteCredentialPage> {
  return invoke("list_route_credentials_page", { input });
}

export function reorderRouteCredentials(input: ReorderRouteCredentialInput): Promise<RouteCredentialPage> {
  return invoke("reorder_route_credentials", { input });
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

export function copyRouteCredential(
  id: string,
  input?: CopyRouteCredentialInput,
): Promise<RouteCredential> {
  return invoke("copy_route_credential", input ? { id, input } : { id });
}

export function setRouteCredentialRecovery(
  id: string,
  rule: RecoveryRule,
): Promise<RouteCredential> {
  return invoke("set_route_credential_recovery", { id, rule });
}

export function deleteRouteCredential(id: string): Promise<void> {
  return invoke("delete_route_credential", { id });
}

export function archiveRouteCredentials(ids: string[]): Promise<void> {
  return invoke("archive_route_credentials", { ids });
}

export function restoreRouteCredentials(ids: string[]): Promise<void> {
  return invoke("restore_route_credentials", { ids });
}

export function setRouteCredentialStatuses(
  ids: string[],
  status: AccountStatus,
): Promise<void> {
  return invoke("set_route_credential_statuses", { ids, status });
}

export function refreshRouteCredentialQuota(id: string): Promise<QuotaRefreshOutcome> {
  return invoke("refresh_route_credential_quota", { id });
}

export function refreshRouteCredentialsQuota(platform: string): Promise<QuotaRefreshOutcome[]> {
  return invoke("refresh_route_credentials_quota", { platform });
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

export function openSessionTerminal(input: { cwd: string; command: string }): Promise<void> {
  return invoke("open_session_terminal", { input });
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

export function listAgentLaunchOptions(): Promise<AgentLaunchOption[]> {
  return invoke("list_agent_launch_options");
}

export function getWebServiceConfig(): Promise<WebServiceConfig> {
  return invoke("get_web_service_config");
}

export function saveWebServiceConfig(config: WebServiceConfig): Promise<WebServiceConfig> {
  return invoke("save_web_service_config", { config });
}

export function getWebServerStatus(): Promise<WebServerStatus> {
  return invoke("get_web_server_status");
}

export function startWebServer(): Promise<WebServerStatus> {
  return invoke("start_web_server");
}

export function stopWebServer(): Promise<WebServerStatus> {
  return invoke("stop_web_server");
}

export function getTailscaleStatus(): Promise<TailscaleStatus> {
  return invoke("get_tailscale_status");
}

export function createMobilePairing(): Promise<MobilePairingPayload> {
  return invoke("create_mobile_pairing");
}

export function startTailscaleLogin(): Promise<TailscaleLogin> {
  return invoke("start_tailscale_login");
}

export function startTailscaleWithAuthKey(authKey: string): Promise<TailscaleStatus> {
  return invoke("start_tailscale_with_auth_key", { authKey });
}

export function disconnectTailscale(): Promise<TailscaleStatus> {
  return invoke("disconnect_tailscale");
}

export function mcpScanLocal(): Promise<LocalMcpServer[]> {
  return invoke("mcp_scan_local");
}

export function mcpListMarketplaces(): Promise<McpMarketplaceProvider[]> {
  return invoke("mcp_list_marketplaces");
}

export function mcpSearchMarketplace(input: {
  providerId: string;
  query?: string | null;
  limit?: number | null;
}): Promise<McpMarketplaceItem[]> {
  return invoke("mcp_search_marketplace", {
    providerId: input.providerId,
    query: input.query ?? null,
    limit: input.limit ?? null,
  });
}

export function mcpGetMarketplaceServerDetail(
  providerId: string,
  serverId: string,
): Promise<McpMarketplaceServerDetail> {
  return invoke("mcp_get_marketplace_server_detail", { providerId, serverId });
}

export function mcpInstallFromMarketplace(input: {
  providerId: string;
  serverId: string;
  apps: McpAppType[];
  optionId?: string | null;
  protocol?: string | null;
  parameterValues?: Record<string, unknown> | null;
}): Promise<LocalMcpServer> {
  return invoke("mcp_install_from_marketplace", {
    providerId: input.providerId,
    serverId: input.serverId,
    apps: input.apps,
    optionId: input.optionId ?? null,
    protocol: input.protocol ?? null,
    parameterValues: input.parameterValues ?? null,
  });
}

export function mcpUpsertLocalServer(input: {
  serverId: string;
  spec: McpSpec;
  apps: McpAppType[];
}): Promise<LocalMcpServer> {
  return invoke("mcp_upsert_local_server", input);
}

export function mcpSetServerApps(serverId: string, apps: McpAppType[]): Promise<LocalMcpServer | null> {
  return invoke("mcp_set_server_apps", { serverId, apps });
}

export function mcpRemoveServer(serverId: string, apps?: McpAppType[] | null): Promise<boolean> {
  return invoke("mcp_remove_server", { serverId, apps: apps ?? null });
}

export function skillsListAgents(): Promise<SkillAgentInfo[]> {
  return invoke("skills_list_agents");
}

export function skillsList(input: {
  agentType: SkillAgentType;
  scope: SkillScope;
  workspacePath?: string | null;
}): Promise<SkillsListResult> {
  return invoke("skills_list", { ...input, workspacePath: input.workspacePath ?? null });
}

export function skillsRead(input: {
  agentType: SkillAgentType;
  scope: SkillScope;
  skillId: string;
  workspacePath?: string | null;
}): Promise<SkillContent> {
  return invoke("skills_read", { ...input, workspacePath: input.workspacePath ?? null });
}

export function skillsSave(input: {
  agentType: SkillAgentType;
  scope: SkillScope;
  skillId: string;
  content: string;
  layout?: SkillLayout | null;
  workspacePath?: string | null;
}): Promise<SkillItem> {
  return invoke("skills_save", { ...input, workspacePath: input.workspacePath ?? null });
}

export function skillsDelete(input: {
  agentType: SkillAgentType;
  scope: SkillScope;
  skillId: string;
  workspacePath?: string | null;
}): Promise<boolean> {
  return invoke("skills_delete", { ...input, workspacePath: input.workspacePath ?? null });
}

export function skillsListPackages(input?: {
  agentType?: SkillAgentType;
  scope?: SkillScope;
  workspacePath?: string | null;
}): Promise<SkillsPackageListResult> {
  return invoke("skills_list_packages", {
    agentType: input?.agentType ?? null,
    scope: input?.scope ?? null,
    workspacePath: input?.workspacePath ?? null,
  });
}

export function skillsReadPackage(input: {
  packageId: string;
  agentType?: SkillAgentType;
  scope?: SkillScope;
  workspacePath?: string | null;
}): Promise<SkillPackageDetail> {
  return invoke("skills_read_package", {
    packageId: input.packageId,
    agentType: input.agentType ?? null,
    scope: input.scope ?? null,
    workspacePath: input.workspacePath ?? null,
  });
}

export function skillsInstallPackage(input: {
  packageId: string;
  agentType?: SkillAgentType;
  scope?: SkillScope;
  workspacePath?: string | null;
}): Promise<SkillPackageInstallResult> {
  return invoke("skills_install_package", {
    packageId: input.packageId,
    agentType: input.agentType ?? null,
    scope: input.scope ?? null,
    workspacePath: input.workspacePath ?? null,
  });
}
