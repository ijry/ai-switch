export type ApiError = {
  code: string;
  message: string;
  details?: string | null;
  recoverable: boolean;
  operation_id?: string | null;
};

export type Batch = {
  id: string;
  name: string;
  source: string;
  notes?: string | null;
  sort_order: number;
  created_at: string;
  updated_at: string;
};

export type BatchChild = {
  item_type: "provider" | "official_account";
  id: string;
  title: string;
  subtitle?: string | null;
  status: "ok" | "warning" | "error";
};

export type BatchGroup = {
  batch: Batch;
  health: "ok" | "warning" | "error";
  children: BatchChild[];
};

export type Provider = {
  id: string;
  name: string;
  kind: string;
  base_url?: string | null;
  model_config_json: string;
  target_options_json: string;
  secret_ref?: string | null;
  status: string;
  sort_order: number;
  created_at: string;
  updated_at: string;
};

export type ProviderPreset = {
  id: string;
  name: string;
  description: string;
  kind: string;
  base_url?: string | null;
  model_config_json: string;
  target_options_json: string;
  secret_env_key?: string | null;
};

export type CreateProviderFromPresetRequest = {
  preset_id: string;
  batch_name?: string | null;
};

export type CreateProviderFromPresetOutcome = {
  provider: Provider;
  batch_id?: string | null;
};

export type OfficialAccount = {
  id: string;
  platform: string;
  display_name: string;
  email?: string | null;
  plan?: string | null;
  account_metadata_json: string;
  secret_ref?: string | null;
  quota_snapshot_id?: string | null;
  status: string;
  sort_order: number;
  created_at: string;
  updated_at: string;
};

export type QuotaSnapshot = {
  id: string;
  owner_type: string;
  owner_id: string;
  status: string;
  remaining_label?: string | null;
  reset_at?: string | null;
  summary_json: string;
  raw_excerpt_json: string;
  fetched_at: string;
};

export type OfficialAccountStatus = {
  account: OfficialAccount;
  quota_snapshot?: QuotaSnapshot | null;
};

export type NewOfficialAccount = {
  platform: string;
  display_name: string;
  email?: string | null;
  plan?: string | null;
  account_metadata_json: string;
  secret_ref?: string | null;
};

export type CreateOfficialAccountRequest = {
  account: NewOfficialAccount;
  batch_id?: string | null;
};

export type RecordAccountQuotaSnapshotRequest = {
  account_id: string;
  status: "ok" | "warning" | "error" | "unknown";
  remaining_label?: string | null;
  reset_at?: string | null;
  summary_json: string;
  raw_excerpt_json: string;
};

export type RefreshAccountQuotaSnapshotRequest = {
  account_id: string;
};

export type RecordAccountQuotaSnapshotOutcome = {
  account: OfficialAccount;
  quota_snapshot: QuotaSnapshot;
};

export type ImportJob = {
  id: string;
  source_type: string;
  source_label: string;
  batch_id?: string | null;
  strategy: string;
  status: string;
  success_count: number;
  failure_count: number;
  conflict_count: number;
  summary_json: string;
  created_at: string;
  completed_at?: string | null;
};

export type OfficialAccountJsonImportRequest = {
  batch_name: string;
  source_label: string;
  platform: "codex" | "claude" | "gemini" | "cursor" | "windsurf" | "zed" | "vscode";
  json: string;
};

export type DeepLinkImportRequest = {
  url: string;
};

export type ExampleJsonExportOutcome = {
  json: string;
  provider_count: number;
  account_count: number;
};

export type TrayMenuStatus = {
  provider_count: number;
  target_count: number;
  switch_item_count: number;
};

export type McpTransport = "stdio" | "sse" | "streamable_http";

export type McpServer = {
  id: string;
  name: string;
  transport: McpTransport;
  command?: string | null;
  args_json: string;
  url?: string | null;
  env_json: string;
  enabled: number;
  notes?: string | null;
  status: string;
  sort_order: number;
  created_at: string;
  updated_at: string;
};

export type NewMcpServer = {
  name: string;
  transport: McpTransport;
  command?: string | null;
  args_json: string;
  url?: string | null;
  env_json: string;
  enabled: boolean;
  notes?: string | null;
};

export type SetMcpServerEnabledRequest = {
  id: string;
  enabled: boolean;
};

export type PromptAssetType = "prompt" | "skill";

export type PromptAsset = {
  id: string;
  item_type: PromptAssetType;
  name: string;
  description?: string | null;
  body: string;
  tags_json: string;
  metadata_json: string;
  enabled: number;
  status: string;
  sort_order: number;
  created_at: string;
  updated_at: string;
};

export type NewPromptAsset = {
  item_type: PromptAssetType;
  name: string;
  description?: string | null;
  body: string;
  tags_json: string;
  metadata_json: string;
  enabled: boolean;
};

export type SetPromptAssetEnabledRequest = {
  id: string;
  enabled: boolean;
};

export type ProxyProfile = {
  id: string;
  name: string;
  endpoint_url: string;
  auth_ref?: string | null;
  enabled: number;
  notes?: string | null;
  status: string;
  sort_order: number;
  created_at: string;
  updated_at: string;
};

export type NewProxyProfile = {
  name: string;
  endpoint_url: string;
  auth_ref?: string | null;
  enabled: boolean;
  notes?: string | null;
};

export type FailoverPolicy = {
  id: string;
  name: string;
  strategy: "ordered" | "round_robin";
  provider_ids_json: string;
  enabled: number;
  notes?: string | null;
  status: string;
  sort_order: number;
  created_at: string;
  updated_at: string;
};

export type NewFailoverPolicy = {
  name: string;
  strategy: "ordered" | "round_robin";
  provider_ids_json: string;
  enabled: boolean;
  notes?: string | null;
};

export type UsageEvent = {
  id: string;
  provider_id?: string | null;
  official_account_id?: string | null;
  source_label: string;
  metric_type: string;
  amount: number;
  unit: string;
  metadata_json: string;
  created_at: string;
};

export type NewUsageEvent = {
  provider_id?: string | null;
  official_account_id?: string | null;
  source_label: string;
  metric_type: string;
  amount: number;
  unit: string;
  metadata_json: string;
};

export type SyncProfileProvider = "local_folder" | "webdav" | "s3" | "git";

export type SyncProfile = {
  id: string;
  name: string;
  provider: SyncProfileProvider;
  endpoint_url?: string | null;
  auth_ref?: string | null;
  scope_json: string;
  enabled: number;
  notes?: string | null;
  status: string;
  sort_order: number;
  created_at: string;
  updated_at: string;
};

export type NewSyncProfile = {
  name: string;
  provider: SyncProfileProvider;
  endpoint_url?: string | null;
  auth_ref?: string | null;
  scope_json: string;
  enabled: boolean;
  notes?: string | null;
};

export type SyncSnapshot = {
  id: string;
  profile_id?: string | null;
  direction: "export" | "import";
  status: string;
  item_counts_json: string;
  manifest_json: string;
  artifact_ref?: string | null;
  created_at: string;
};

export type CreateSyncSnapshotRequest = {
  profile_id?: string | null;
  direction: "export" | "import";
  artifact_ref?: string | null;
};

export type SessionStatus = "draft" | "active" | "archived";

export type SessionRecord = {
  id: string;
  title: string;
  target_app_id?: string | null;
  provider_id?: string | null;
  official_account_id?: string | null;
  prompt_asset_id?: string | null;
  mcp_server_ids_json: string;
  tags_json: string;
  status: SessionStatus;
  notes?: string | null;
  sort_order: number;
  created_at: string;
  updated_at: string;
};

export type NewSessionRecord = {
  title: string;
  target_app_id?: string | null;
  provider_id?: string | null;
  official_account_id?: string | null;
  prompt_asset_id?: string | null;
  mcp_server_ids_json: string;
  tags_json: string;
  status: SessionStatus;
  notes?: string | null;
};

export type SetSessionStatusRequest = {
  id: string;
  status: SessionStatus;
};

export type SessionEventType =
  | "note"
  | "status"
  | "usage"
  | "quota"
  | "error"
  | "import"
  | "switch";

export type SessionEvent = {
  id: string;
  session_id: string;
  event_type: SessionEventType;
  message: string;
  metadata_json: string;
  created_at: string;
};

export type NewSessionEvent = {
  session_id: string;
  event_type: SessionEventType;
  message: string;
  metadata_json: string;
};

export type ListSessionEventsRequest = {
  session_id?: string | null;
};

export type UpdateChannelName = "stable" | "beta" | "nightly";

export type UpdateChannel = {
  id: string;
  name: string;
  channel: UpdateChannelName;
  feed_url?: string | null;
  enabled: number;
  notes?: string | null;
  status: string;
  sort_order: number;
  created_at: string;
  updated_at: string;
};

export type NewUpdateChannel = {
  name: string;
  channel: UpdateChannelName;
  feed_url?: string | null;
  enabled: boolean;
  notes?: string | null;
};

export type UpdateCheckStatus = "unknown" | "up_to_date" | "available" | "error";

export type UpdateCheck = {
  id: string;
  channel_id?: string | null;
  current_version: string;
  latest_version?: string | null;
  status: UpdateCheckStatus;
  release_notes_url?: string | null;
  details_json: string;
  checked_at: string;
};

export type NewUpdateCheck = {
  channel_id?: string | null;
  current_version: string;
  latest_version?: string | null;
  status: UpdateCheckStatus;
  release_notes_url?: string | null;
  details_json: string;
};

export type InstanceStatus = "configured" | "running" | "stopped" | "error";

export type ManagedInstance = {
  id: string;
  name: string;
  target_app_id?: string | null;
  provider_id?: string | null;
  launch_args_json: string;
  env_json: string;
  profile_json: string;
  status: InstanceStatus;
  notes?: string | null;
  sort_order: number;
  created_at: string;
  updated_at: string;
};

export type NewManagedInstance = {
  name: string;
  target_app_id?: string | null;
  provider_id?: string | null;
  launch_args_json: string;
  env_json: string;
  profile_json: string;
  status: InstanceStatus;
  notes?: string | null;
};

export type SetInstanceStatusRequest = {
  id: string;
  status: InstanceStatus;
};

export type WakeupTriggerType = "manual" | "scheduled" | "interval";

export type WakeupTaskStatus = "configured" | "paused" | "error";

export type WakeupTask = {
  id: string;
  name: string;
  managed_instance_id?: string | null;
  target_app_id?: string | null;
  provider_id?: string | null;
  trigger_type: WakeupTriggerType;
  schedule_json: string;
  action_json: string;
  enabled: number;
  status: WakeupTaskStatus;
  last_run_at?: string | null;
  notes?: string | null;
  sort_order: number;
  created_at: string;
  updated_at: string;
};

export type NewWakeupTask = {
  name: string;
  managed_instance_id?: string | null;
  target_app_id?: string | null;
  provider_id?: string | null;
  trigger_type: WakeupTriggerType;
  schedule_json: string;
  action_json: string;
  enabled: boolean;
  status: WakeupTaskStatus;
  notes?: string | null;
};

export type SetWakeupTaskEnabledRequest = {
  id: string;
  enabled: boolean;
};

export type WakeupRunOutcome = "recorded" | "skipped" | "failed";

export type WakeupRun = {
  id: string;
  task_id: string;
  outcome: WakeupRunOutcome;
  message: string;
  metadata_json: string;
  created_at: string;
};

export type NewWakeupRun = {
  task_id: string;
  outcome: WakeupRunOutcome;
  message: string;
  metadata_json: string;
};

export type ListWakeupRunsRequest = {
  task_id?: string | null;
};

export type AutomationItemType =
  | "provider"
  | "official_account"
  | "mcp_server"
  | "prompt_asset"
  | "session"
  | "managed_instance"
  | "wakeup_task"
  | "target_app"
  | "mixed";

export type TagRecord = {
  id: string;
  name: string;
  color?: string | null;
  description?: string | null;
  sort_order: number;
  created_at: string;
  updated_at: string;
};

export type NewTagRecord = {
  name: string;
  color?: string | null;
  description?: string | null;
};

export type ItemTag = {
  id: string;
  tag_id: string;
  item_type: AutomationItemType;
  item_id: string;
  created_at: string;
};

export type NewItemTag = {
  tag_id: string;
  item_type: AutomationItemType;
  item_id: string;
};

export type PluginLinkStatus = "configured" | "paused" | "error";

export type PluginLink = {
  id: string;
  name: string;
  plugin_key: string;
  item_type: AutomationItemType;
  item_id: string;
  config_json: string;
  enabled: number;
  status: PluginLinkStatus;
  notes?: string | null;
  sort_order: number;
  created_at: string;
  updated_at: string;
};

export type NewPluginLink = {
  name: string;
  plugin_key: string;
  item_type: AutomationItemType;
  item_id: string;
  config_json: string;
  enabled: boolean;
  status: PluginLinkStatus;
  notes?: string | null;
};

export type SetPluginLinkEnabledRequest = {
  id: string;
  enabled: boolean;
};

export type BulkOperationType =
  | "tag_apply"
  | "tag_remove"
  | "status_record"
  | "export_selection"
  | "plugin_link";

export type BulkOperationStatus = "planned" | "recorded" | "cancelled" | "error";

export type BulkOperation = {
  id: string;
  name: string;
  operation_type: BulkOperationType;
  target_type: AutomationItemType;
  item_ids_json: string;
  parameters_json: string;
  dry_run: number;
  status: BulkOperationStatus;
  summary_json: string;
  created_at: string;
  updated_at: string;
};

export type NewBulkOperation = {
  name: string;
  operation_type: BulkOperationType;
  target_type: AutomationItemType;
  item_ids_json: string;
  parameters_json: string;
  dry_run: boolean;
  status: BulkOperationStatus;
  summary_json: string;
};

export type TargetApp = {
  id: string;
  key: string;
  display_name: string;
  enabled: number;
  sort_order: number;
  created_at: string;
  updated_at: string;
};

export type TargetSwitchStatus = {
  target: TargetApp;
  active_provider?: Provider | null;
  last_write_status?: string | null;
  last_error_code?: string | null;
  last_written_at?: string | null;
  last_snapshot_path?: string | null;
  last_snapshot_id?: string | null;
  last_snapshot_operation?: string | null;
  can_rollback: boolean;
};

export type ProviderSwitchRequest = {
  target_app_id: string;
  provider_id: string;
  mode: "sandbox" | "real";
};

export type ProviderSwitchOutcome = {
  target_app_id: string;
  target_key: string;
  provider_id: string;
  provider_name: string;
  mode: "sandbox" | "real";
  path: string;
  status: string;
  before_hash?: string | null;
  after_hash?: string | null;
  snapshot_id: string;
  state_id: string;
  written_at: string;
};

export type ConfigRollbackOutcome = {
  target_app_id: string;
  target_key: string;
  source_snapshot_id: string;
  rollback_snapshot_id: string;
  state_id: string;
  path: string;
  status: string;
  before_hash?: string | null;
  after_hash?: string | null;
  rolled_back_at: string;
};

export type AppSettings = {
  language: string;
  theme: string;
  copy_import_sources: boolean;
  logging_enabled: boolean;
  secret_storage: string;
  data_dir: string;
};
