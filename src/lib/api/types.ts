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
};

export type RoutePoolModelTestOutcome = {
  platform: string;
  selected_account_id: string;
  selected_account_name: string;
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

export type RouteConfigWriteOutcome = {
  target_key: string;
  path: string;
  status: string;
  route_proxy_key: string;
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
  display_name: string;
  enabled: number;
  sort_order: number;
  created_at: string;
  updated_at: string;
};

export type AppSettings = {
  language: string;
  theme: string;
  copy_import_sources: boolean;
  logging_enabled: boolean;
  secret_storage: string;
  data_dir: string;
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
