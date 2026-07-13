import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  BulkOperation,
  Batch,
  BatchGroup,
  ConfigRollbackOutcome,
  CreateOfficialAccountRequest,
  CreateProviderFromPresetOutcome,
  CreateProviderFromPresetRequest,
  DeepLinkImportRequest,
  ExampleJsonExportOutcome,
  FailoverPolicy,
  ImportJob,
  ListSessionEventsRequest,
  ListWakeupRunsRequest,
  ItemTag,
  ManagedInstance,
  McpServer,
  NewBulkOperation,
  NewFailoverPolicy,
  NewItemTag,
  NewManagedInstance,
  NewMcpServer,
  NewPluginLink,
  NewSessionEvent,
  NewSessionRecord,
  NewSyncProfile,
  NewTagRecord,
  NewUpdateChannel,
  NewUpdateCheck,
  NewWakeupRun,
  NewWakeupTask,
  NewProxyProfile,
  NewPromptAsset,
  NewUsageEvent,
  OfficialAccount,
  OfficialAccountJsonImportRequest,
  OfficialAccountStatus,
  Provider,
  PluginLink,
  ProxyProfile,
  PromptAsset,
  ProviderPreset,
  RecordAccountQuotaSnapshotOutcome,
  RecordAccountQuotaSnapshotRequest,
  RefreshAccountQuotaSnapshotRequest,
  ProviderSwitchOutcome,
  ProviderSwitchRequest,
  SetMcpServerEnabledRequest,
  SetInstanceStatusRequest,
  SetPluginLinkEnabledRequest,
  SetSessionStatusRequest,
  SetPromptAssetEnabledRequest,
  SetWakeupTaskEnabledRequest,
  CreateSyncSnapshotRequest,
  SessionEvent,
  SessionRecord,
  SyncProfile,
  SyncSnapshot,
  TagRecord,
  TargetApp,
  TargetSwitchStatus,
  TrayMenuStatus,
  UpdateChannel,
  UpdateCheck,
  UsageEvent,
  WakeupRun,
  WakeupTask,
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

export function importExampleJson(request: {
  batch_name: string;
  source_label: string;
  strategy: string;
  json: string;
}): Promise<ImportJob> {
  return invoke("import_example_json", { request });
}

export function importOfficialAccountJson(
  request: OfficialAccountJsonImportRequest,
): Promise<ImportJob> {
  return invoke("import_official_account_json", { request });
}

export function importDeepLink(request: DeepLinkImportRequest): Promise<ImportJob> {
  return invoke("import_deep_link", { request });
}

export function exportExampleJson(): Promise<ExampleJsonExportOutcome> {
  return invoke("export_example_json");
}

export function listTargetApps(): Promise<TargetApp[]> {
  return invoke("list_target_apps");
}

export function listProviders(): Promise<Provider[]> {
  return invoke("list_providers");
}

export function listOfficialAccounts(): Promise<OfficialAccount[]> {
  return invoke("list_official_accounts");
}

export function listOfficialAccountStatuses(): Promise<OfficialAccountStatus[]> {
  return invoke("list_official_account_statuses");
}

export function listProviderPresets(): Promise<ProviderPreset[]> {
  return invoke("list_provider_presets");
}

export function createProviderFromPreset(
  request: CreateProviderFromPresetRequest,
): Promise<CreateProviderFromPresetOutcome> {
  return invoke("create_provider_from_preset", { request });
}

export function createOfficialAccount(
  request: CreateOfficialAccountRequest,
): Promise<OfficialAccount> {
  return invoke("create_official_account", { request });
}

export function recordOfficialAccountQuotaSnapshot(
  request: RecordAccountQuotaSnapshotRequest,
): Promise<RecordAccountQuotaSnapshotOutcome> {
  return invoke("record_official_account_quota_snapshot", { request });
}

export function refreshOfficialAccountQuotaSnapshot(
  request: RefreshAccountQuotaSnapshotRequest,
): Promise<RecordAccountQuotaSnapshotOutcome> {
  return invoke("refresh_official_account_quota_snapshot", { request });
}

export function listTargetSwitchStatuses(): Promise<TargetSwitchStatus[]> {
  return invoke("list_target_switch_statuses");
}

export function rollbackConfigSnapshot(snapshotId: string): Promise<ConfigRollbackOutcome> {
  return invoke("rollback_config_snapshot", { snapshotId });
}

export function switchTargetProvider(
  request: ProviderSwitchRequest,
): Promise<ProviderSwitchOutcome> {
  return invoke("switch_target_provider", { request });
}

export function refreshTrayMenu(): Promise<TrayMenuStatus> {
  return invoke("refresh_tray_menu");
}

export function listTags(): Promise<TagRecord[]> {
  return invoke("list_tags");
}

export function createTag(request: NewTagRecord): Promise<TagRecord> {
  return invoke("create_tag", { request });
}

export function listItemTags(): Promise<ItemTag[]> {
  return invoke("list_item_tags");
}

export function createItemTag(request: NewItemTag): Promise<ItemTag> {
  return invoke("create_item_tag", { request });
}

export function listPluginLinks(): Promise<PluginLink[]> {
  return invoke("list_plugin_links");
}

export function createPluginLink(request: NewPluginLink): Promise<PluginLink> {
  return invoke("create_plugin_link", { request });
}

export function setPluginLinkEnabled(
  request: SetPluginLinkEnabledRequest,
): Promise<PluginLink> {
  return invoke("set_plugin_link_enabled", { request });
}

export function listBulkOperations(): Promise<BulkOperation[]> {
  return invoke("list_bulk_operations");
}

export function createBulkOperation(
  request: NewBulkOperation,
): Promise<BulkOperation> {
  return invoke("create_bulk_operation", { request });
}

export function listMcpServers(): Promise<McpServer[]> {
  return invoke("list_mcp_servers");
}

export function createMcpServer(request: NewMcpServer): Promise<McpServer> {
  return invoke("create_mcp_server", { request });
}

export function setMcpServerEnabled(
  request: SetMcpServerEnabledRequest,
): Promise<McpServer> {
  return invoke("set_mcp_server_enabled", { request });
}

export function listPromptAssets(): Promise<PromptAsset[]> {
  return invoke("list_prompt_assets");
}

export function createPromptAsset(request: NewPromptAsset): Promise<PromptAsset> {
  return invoke("create_prompt_asset", { request });
}

export function setPromptAssetEnabled(
  request: SetPromptAssetEnabledRequest,
): Promise<PromptAsset> {
  return invoke("set_prompt_asset_enabled", { request });
}

export function listProxyProfiles(): Promise<ProxyProfile[]> {
  return invoke("list_proxy_profiles");
}

export function createProxyProfile(request: NewProxyProfile): Promise<ProxyProfile> {
  return invoke("create_proxy_profile", { request });
}

export function listFailoverPolicies(): Promise<FailoverPolicy[]> {
  return invoke("list_failover_policies");
}

export function createFailoverPolicy(request: NewFailoverPolicy): Promise<FailoverPolicy> {
  return invoke("create_failover_policy", { request });
}

export function listUsageEvents(): Promise<UsageEvent[]> {
  return invoke("list_usage_events");
}

export function createUsageEvent(request: NewUsageEvent): Promise<UsageEvent> {
  return invoke("create_usage_event", { request });
}

export function listSyncProfiles(): Promise<SyncProfile[]> {
  return invoke("list_sync_profiles");
}

export function createSyncProfile(request: NewSyncProfile): Promise<SyncProfile> {
  return invoke("create_sync_profile", { request });
}

export function listSyncSnapshots(): Promise<SyncSnapshot[]> {
  return invoke("list_sync_snapshots");
}

export function createSyncSnapshot(
  request: CreateSyncSnapshotRequest,
): Promise<SyncSnapshot> {
  return invoke("create_sync_snapshot", { request });
}

export function listSessions(): Promise<SessionRecord[]> {
  return invoke("list_sessions");
}

export function createSession(request: NewSessionRecord): Promise<SessionRecord> {
  return invoke("create_session", { request });
}

export function setSessionStatus(
  request: SetSessionStatusRequest,
): Promise<SessionRecord> {
  return invoke("set_session_status", { request });
}

export function listSessionEvents(
  request: ListSessionEventsRequest,
): Promise<SessionEvent[]> {
  return invoke("list_session_events", { request });
}

export function createSessionEvent(request: NewSessionEvent): Promise<SessionEvent> {
  return invoke("create_session_event", { request });
}

export function listUpdateChannels(): Promise<UpdateChannel[]> {
  return invoke("list_update_channels");
}

export function createUpdateChannel(
  request: NewUpdateChannel,
): Promise<UpdateChannel> {
  return invoke("create_update_channel", { request });
}

export function listUpdateChecks(): Promise<UpdateCheck[]> {
  return invoke("list_update_checks");
}

export function createUpdateCheck(request: NewUpdateCheck): Promise<UpdateCheck> {
  return invoke("create_update_check", { request });
}

export function listInstances(): Promise<ManagedInstance[]> {
  return invoke("list_instances");
}

export function createInstance(request: NewManagedInstance): Promise<ManagedInstance> {
  return invoke("create_instance", { request });
}

export function setInstanceStatus(
  request: SetInstanceStatusRequest,
): Promise<ManagedInstance> {
  return invoke("set_instance_status", { request });
}

export function listWakeupTasks(): Promise<WakeupTask[]> {
  return invoke("list_wakeup_tasks");
}

export function createWakeupTask(request: NewWakeupTask): Promise<WakeupTask> {
  return invoke("create_wakeup_task", { request });
}

export function setWakeupTaskEnabled(
  request: SetWakeupTaskEnabledRequest,
): Promise<WakeupTask> {
  return invoke("set_wakeup_task_enabled", { request });
}

export function listWakeupRuns(
  request: ListWakeupRunsRequest,
): Promise<WakeupRun[]> {
  return invoke("list_wakeup_runs", { request });
}

export function createWakeupRun(request: NewWakeupRun): Promise<WakeupRun> {
  return invoke("create_wakeup_run", { request });
}

export function getSettings(): Promise<AppSettings> {
  return invoke("get_settings");
}

export function saveSettings(settings: AppSettings): Promise<AppSettings> {
  return invoke("save_settings", { settings });
}
