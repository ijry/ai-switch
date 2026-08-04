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

export type AccountStatus = "ok" | "warning" | "error" | "revoked";

export type RouteCredentialKind = "official" | "api";

export type PlatformId =
  | "codex"
  | "claude"
  | "gemini"
  | "grok"
  | "opencode"
  | "openclaw"
  | "hermes";

export type ApiDialect = "openai" | "openai-responses" | "anthropic" | "gemini";

export type CapabilityAvailability = "supported" | "partial" | "unavailable";

export type PlatformSupportLevel = "supported" | "partial";

export type CapabilityRule = {
  availability: CapabilityAvailability;
  reason_code?: string | null;
  credential_kinds: string[];
  requires_base_url: boolean;
  requires_api_dialect: boolean;
};

export type PlatformOperations = {
  route_credentials: CapabilityRule;
  generic_api_routing: CapabilityRule;
  config_write: CapabilityRule;
  official_import: CapabilityRule;
  official_account_routing: CapabilityRule;
  deeplink_import: CapabilityRule;
  official_quota: CapabilityRule;
  model_test: CapabilityRule;
  terminal_launch: CapabilityRule;
  session_resume: CapabilityRule;
};

export type PlatformCapability = {
  platform: PlatformId;
  display_name: string;
  support_level: PlatformSupportLevel;
  operations: PlatformOperations;
};

export type InterfaceFormat =
  | "openai"
  | "openai-responses"
  | "anthropic"
  | "anthropic-messages"
  | "gemini";

export type AnthropicApiKeyField = "ANTHROPIC_API_KEY" | "ANTHROPIC_AUTH_TOKEN";

export type BatchChild = {
  item_type: "provider" | "official_account";
  id: string;
  title: string;
  subtitle?: string | null;
  platform?: string | null;
  status: AccountStatus;
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

export type OfficialAccount = {
  id: string;
  platform: string;
  display_name: string;
  email?: string | null;
  plan?: string | null;
  account_metadata_json: string;
  secret_ref?: string | null;
  quota_snapshot_id?: string | null;
  status: AccountStatus;
  sort_order: number;
  created_at: string;
  updated_at: string;
};

export type NewOfficialAccount = {
  platform: string;
  display_name: string;
  email?: string | null;
  plan?: string | null;
  account_metadata_json: string;
  secret_ref?: string | null;
};

export type UpdateOfficialAccount = {
  display_name: string;
  email?: string | null;
  plan?: string | null;
  account_metadata_json: string;
  secret_ref?: string | null;
  status: AccountStatus;
};

export type ModelMapping = {
  from: string;
  to: string;
  label?: string | null;
  supports_1m?: boolean | null;
};

export type RouteModelsFetchRequest = {
  base_url: string;
  api_key: string;
  interface_format?: InterfaceFormat | string | null;
  api_key_field?: AnthropicApiKeyField | string | null;
};

export type FetchedRouteModel = {
  id: string;
  owned_by?: string | null;
  supports_1m?: boolean | null;
};

export type RouteCredential = {
  id: string;
  platform: string;
  kind: RouteCredentialKind;
  display_name: string;
  email?: string | null;
  status: AccountStatus;
  sort_order: number;
  batch_id?: string | null;
  batch_name?: string | null;
  secret_payload_json: string;
  config_json: string;
  preview_json: string;
  subscription_type?: string | null;
  primary_remain?: number | null;
  weekly_remain?: number | null;
  reset_primary?: string | null;
  reset_weekly?: string | null;
  quota_remaining?: number | null;
  quota_limit?: number | null;
  quota_used?: number | null;
  quota_updated_at?: string | null;
  transient_failure_count?: number;
  next_retry_at?: string | null;
  cooldown_until?: string | null;
  last_failure_kind?: string | null;
  last_failure_message?: string | null;
  request_count?: number;
  success_count?: number;
  failure_count?: number;
  success_rate?: number | null;
  created_at: string;
  updated_at: string;
};

export type QuotaRefreshOutcome = {
  credential: RouteCredential;
  updated: boolean;
  source: string;
  message?: string | null;
};

export type CreateApiRouteCredentialInput = {
  platform: string;
  display_name: string;
  api_key: string;
  base_url: string;
  interface_format: InterfaceFormat;
  model_mappings_json: string;
  api_key_field?: AnthropicApiKeyField | string | null;
  preview_json?: string | null;
  batch_id?: string | null;
  responses_custom_tool_compat?: boolean | null;
  user_agent?: string | null;
};

export type UpdateRouteCredentialInput = {
  display_name: string;
  email?: string | null;
  status: AccountStatus;
  secret_payload_json: string;
  config_json: string;
  preview_json: string;
};

export type RouteCredentialImportResult = {
  imported: RouteCredential[];
  failed: Array<{ label: string; error: string }>;
};

export type RouteCredentialFilterOption = {
  key: string;
  label: string;
};

export type RouteCredentialPoolScope = "in_pool" | "out_of_pool";

export type RouteCredentialPage = {
  items: RouteCredential[];
  total: number;
  page: number;
  page_count: number;
  page_size: number;
  previous_page_account_id?: string | null;
  next_page_account_id?: string | null;
  filter_options: RouteCredentialFilterOption[];
  official_account_count: number;
};

export type RouteCredentialPageRequest = {
  platform: string;
  page: number;
  page_size: number;
  filters: string[];
  pool_scope: RouteCredentialPoolScope;
};

export type ReorderRouteCredentialInput = {
  platform: string;
  moved_account_id: string;
  previous_account_id?: string | null;
  next_account_id?: string | null;
  filters: string[];
  pool_scope: RouteCredentialPoolScope;
  page_size: number;
};

export type RoutePoolUsageLog = {
  id: string;
  account_id?: string | null;
  account_name?: string | null;
  source_label: string;
  metric_type: string;
  amount: number;
  unit: string;
  metadata_json: string;
  created_at: string;
};

export type RoutePoolStats = {
  member_count: number;
  request_count: number;
  token_count: number;
  cost_micros: number;
  recent_logs: RoutePoolUsageLog[];
  requests: RoutePoolUsageLog[];
  request_row_count: number;
  request_page: number;
  request_page_size: number;
};

export type RoutePoolState = {
  platform: string;
  account_ids: string[];
  stats: RoutePoolStats;
};

export type RoutePoolRouteRequest = {
  platform: string;
  token_count?: number | null;
  cost_micros?: number | null;
  metadata_json?: string | null;
};

export type RoutePoolRouteOutcome = {
  platform: string;
  selected_account_id: string;
  selected_account_name: string;
  stats: RoutePoolStats;
};

export type RoutePoolModelTestRequest = {
  platform: string;
  account_id?: string | null;
  model?: string | null;
  interface_format?: "openai" | "openai-responses" | null;
};

export type RoutePoolModelTestOutcome = {
  platform: string;
  selected_account_id: string;
  selected_account_name: string;
  via_route_proxy?: boolean;
  route_proxy_entry_url?: string | null;
  route_proxy_entry_path?: string | null;
  route_proxy_trace_id?: string | null;
  interface_format: string;
  request_path: string;
  base_url?: string | null;
  target_url?: string | null;
  request_body_json: string;
  response_status?: number | null;
  response_body: string;
  response_text?: string | null;
  error_message?: string | null;
  success: boolean;
  duration_ms: number;
  stats: RoutePoolStats;
};

export type RouteProxyStatus = {
  running: boolean;
  bind_host: string;
  port?: number | null;
  base_url?: string | null;
};

export type RouteProxyTrustStatus =
  | "systemTrusted"
  | "nssTrusted"
  | "partiallyTrusted"
  | "untrusted"
  | "unknown";

export type RouteProxyHttpsStatus = {
  enabled: boolean;
  certReady: boolean;
  trustStatus: RouteProxyTrustStatus;
  trustAdapter?: string | null;
  rootFingerprint?: string | null;
  expiresAt?: string | null;
  certificateDir: string;
  rootCertificatePath?: string | null;
  proxyBaseUrl?: string | null;
  message?: string | null;
  manualInstructions: string[];
};

export type WebServiceConfig = {
  host: string;
  port: number;
  token?: string | null;
  autoStart: boolean;
  tailscaleEnabled: boolean;
  tailscaleHostname?: string | null;
  tailscaleAuthKeyPresent?: boolean;
  /** private = tailnet only; public = Tailscale Funnel */
  tailscaleExposureMode?: "private" | "public";
};

export type WebServerStatus = {
  running: boolean;
  host: string;
  port?: number | null;
  baseUrl?: string | null;
};

export type TailscaleStatus = {
  state: string;
  deviceName?: string | null;
  tailnetIp?: string | null;
  magicDnsName?: string | null;
  loginUrl?: string | null;
  accessUrls?: string[];
  serving?: boolean;
  public?: boolean;
  exposureMode?: string | null;
  publicPort?: number | null;
  message?: string | null;
};

export type TailscaleLogin = {
  loginUrl?: string | null;
  message: string;
};

export type ConfigWriteOutcome = {
  operation_id: string;
  snapshot_id?: string | null;
  target_app_id?: string | null;
  target_key: string;
  platform: string;
  path: string;
  status: string;
  before_hash?: string | null;
  after_hash?: string | null;
  error_code?: string | null;
};

export type RouteProxyHttpsOperationOutcome = {
  https: RouteProxyHttpsStatus;
  routeProxy: RouteProxyStatus;
  configWrites: ConfigWriteOutcome[];
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

export type TargetApp = {
  id: string;
  key: string;
  platform?: string | null;
  display_name: string;
  enabled: number;
  sort_order: number;
  created_at: string;
  updated_at: string;
};

export type ConfigSnapshotSummary = {
  id: string;
  target_app_id?: string | null;
  platform?: string | null;
  operation: string;
  operation_group_id?: string | null;
  source_snapshot_id?: string | null;
  path: string;
  before_hash?: string | null;
  after_hash?: string | null;
  original_file_existed: number;
  status: string;
  error_code?: string | null;
  created_at: string;
  updated_at: string;
};

export type TargetConfigStatus = {
  target: TargetApp;
  support_level?: string | null;
  adapter_available: boolean;
  config_path?: string | null;
  file_status: string;
  last_write_status?: string | null;
  last_error_code?: string | null;
  last_written_at?: string | null;
  snapshot_count: number;
  latest_snapshot?: ConfigSnapshotSummary | null;
};

export type AppSettings = {
  language: string;
  theme: string;
  copy_import_sources: boolean;
  logging_enabled: boolean;
  secret_storage: string;
  data_dir: string;
  ccswitch_deeplink_compat_enabled: boolean;
};

export type AppSettingsView = AppSettings & {
  ccswitch_deeplink_compat_supported?: boolean;
};

export type SessionMeta = {
  providerId: string;
  sessionId: string;
  title?: string | null;
  projectDir?: string | null;
  createdAt?: number | null;
  lastActiveAt?: number | null;
  sourcePath: string;
  resumeCommand?: string | null;
};

export type SessionMessage = {
  role: string;
  content: string;
  ts?: number | null;
};

export type TerminalLaunchKind = "shell" | "agent" | "resume";

export type TerminalStatus = "running" | "exited" | "error";

export type CreateTerminalSessionInput = {
  kind: TerminalLaunchKind;
  platform?: string | null;
  command?: string | null;
  title?: string | null;
  cwd: string;
  cols?: number | null;
  rows?: number | null;
};

export type TerminalSession = {
  id: string;
  title: string;
  platform?: string | null;
  cwd: string;
  command: string;
  status: TerminalStatus;
  createdAt: number;
};

export type TerminalOutputEvent = {
  sessionId: string;
  data: string;
};

export type TerminalExitEvent = {
  sessionId: string;
  exitCode?: number | null;
};

export type TerminalErrorEvent = {
  sessionId: string;
  message: string;
};
