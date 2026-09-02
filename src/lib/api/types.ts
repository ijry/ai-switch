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

export type AccountStatus = "ok" | "warning" | "error" | "revoked" | "paused";

export type RouteCredentialKind = "official" | "api";

export type RouteCredentialFailurePolicy = {
  retry_count: number;
  retry_interval_ms: number;
  semantic_error_threshold: number;
  cooldown_enabled: boolean;
  cooldown_seconds: number;
  error_status_enabled: boolean;
};

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

export type FetchedRouteModelReasoningLevel = {
  effort: string;
  description?: string | null;
};

export type FetchedRouteModel = {
  id: string;
  owned_by?: string | null;
  supports_1m?: boolean | null;
  supported_reasoning_levels?: FetchedRouteModelReasoningLevel[];
  default_reasoning_level?: string | null;
};

export type RouteCredentialModelStatus = "ok" | "error" | "paused";

export type RouteCredentialModelState = {
  route_credential_id: string;
  model_key: string;
  aliases: string[];
  status: RouteCredentialModelStatus;
  transient_failure_count: number;
  cooldown_until?: string | null;
  semantic_failure_streak_count: number;
  last_failure_kind?: string | null;
  last_failure_message?: string | null;
  last_failure_response_json?: string | null;
  created_at: string;
  updated_at: string;
};

export type RouteCredential = {
  id: string;
  platform: string;
  kind: RouteCredentialKind;
  display_name: string;
  email?: string | null;
  status: AccountStatus;
  sort_order: number;
  route_priority: number;
  max_concurrency: number;
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
  archived_at?: string | null;
  transient_failure_count?: number;
  next_retry_at?: string | null;
  cooldown_until?: string | null;
  last_failure_kind?: string | null;
  last_failure_message?: string | null;
  last_failure_response_json?: string | null;
  active_request_count?: number;
  model_states?: RouteCredentialModelState[];
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
  fetched_models_json?: string | null;
  api_key_field?: AnthropicApiKeyField | string | null;
  preview_json?: string | null;
  batch_id?: string | null;
  responses_custom_tool_compat?: boolean | null;
  user_agent?: string | null;
};

export type CopyRouteCredentialInput = {
  target_platform: PlatformId;
  api_key?: string | null;
};

export type UpdateRouteCredentialInput = {
  display_name: string;
  email?: string | null;
  status: AccountStatus;
  route_priority: number;
  max_concurrency: number;
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

export type RouteCredentialPoolScope = "in_pool" | "out_of_pool" | "archived";

export type RecoveryMode = "off" | "scheduled" | "healthcheck";

export type RecoveryRule = {
  mode: RecoveryMode;
  times: string[];
  probe_interval_minutes?: number | null;
};

export type TransferPlatformChoice = {
  item_index: number;
  platform: string;
  interface_format?: string | null;
};

export type RouteCredentialSelectionContext = {
  platform: string;
  pool_scope: RouteCredentialPoolScope;
};

export type ExportRouteCredentialsInput = {
  selection_context: RouteCredentialSelectionContext;
  credential_ids: string[];
  include_enhanced_metadata?: boolean;
};

export type RouteCredentialTransferIssue = {
  item_index?: number | null;
  display_name?: string | null;
  code: string;
  field?: string | null;
};

export type RouteCredentialExportCounts = {
  total: number;
  official: number;
  api: number;
};

export type RouteCredentialSchemeLink = {
  credential_id: string;
  display_name: string;
  url?: string | null;
  issue_code?: string | null;
};

export type RouteCredentialExportResult = {
  json_text: string | null;
  suggested_file_name: string;
  counts: RouteCredentialExportCounts;
  scheme_links: RouteCredentialSchemeLink[];
  warnings: RouteCredentialTransferIssue[];
  errors: RouteCredentialTransferIssue[];
};

export type SaveRouteCredentialExportResult = {
  cancelled: boolean;
  file_name?: string | null;
};

export type PreviewRouteCredentialImportInput = {
  text: string;
  ambiguous_platform_choices: TransferPlatformChoice[];
};

export type RouteCredentialImportPreviewItem = {
  item_index: number;
  display_name_masked: string;
  platform?: string | null;
  kind?: string | null;
  cpa_section?: string | null;
  disposition: string;
  issue_codes: string[];
};

export type RouteCredentialImportPreviewCounts = {
  total: number;
  official: number;
  api: number;
  importable: number;
  duplicates: number;
  conflicts: number;
  errors: number;
  restorable_pool_count: number;
  batch_count: number;
  platform_counts: Record<string, number>;
  cpa_section_counts: Record<string, number>;
  legacy_type_counts: Record<string, number>;
  restorable_pool_counts: Record<string, number>;
};

export type RouteCredentialImportPreview = {
  counts: RouteCredentialImportPreviewCounts;
  items: RouteCredentialImportPreviewItem[];
};

export type ImportRouteCredentialsInput = {
  text: string;
  ambiguous_platform_choices: TransferPlatformChoice[];
  restore_pool_membership: boolean;
};

export type RouteCredentialImportOutcome = {
  imported: number;
  skipped_duplicates: number;
  conflicts: number;
  failed: number;
  restored_pool_members: number;
};

/** Third-party desktop clients whose accounts AI Switch can read. */
export type ExternalImportClient = "cc-switch";

export type PreviewExternalClientImportInput = {
  client: ExternalImportClient;
  platform: string;
  /** `null` means "look in the client's default config location". */
  source_path?: string | null;
};

export type ImportExternalClientAccountsInput = {
  client: ExternalImportClient;
  platform: string;
  source_path?: string | null;
  /** The source client's own record ids, taken verbatim from the preview. */
  source_ids: string[];
};

export type ExternalClientAccountPreviewItem = {
  source_id: string;
  display_name: string;
  platform: string;
  interface_format?: string | null;
  base_url?: string | null;
  api_key_masked?: string | null;
  model_mapping_count: number;
  /** `create` | `overwrite` | `error`. */
  disposition: string;
  existing_credential_id?: string | null;
  existing_display_name?: string | null;
  issue_codes: string[];
};

export type ExternalClientImportPreviewCounts = {
  total: number;
  importable: number;
  create: number;
  overwrite: number;
  errors: number;
  other_platform: number;
  other_platform_counts: Record<string, number>;
};

export type ExternalClientImportPreview = {
  client: string;
  source_path: string;
  counts: ExternalClientImportPreviewCounts;
  items: ExternalClientAccountPreviewItem[];
};

export type ExternalClientImportOutcome = {
  created: number;
  overwritten: number;
  skipped: number;
  failed: number;
  imported: RouteCredential[];
  /** Ids of newly created accounts; overwrites keep their existing pool state. */
  created_ids: string[];
};

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
  input_tokens?: number | null;
  output_tokens?: number | null;
  cache_tokens?: number | null;
  price_usd_micros?: number | null;
  price_cny_micros?: number | null;
  price_currency?: "usd" | "cny" | null;
  /**
   * `upstream` when the response carried a real price, `estimated` when it was
   * computed locally from tokens, null when the request has no price at all.
   */
  price_source?: "upstream" | "estimated" | null;
};

export type RoutePoolStats = {
  member_count: number;
  request_count: number;
  token_count: number;
  input_token_count: number;
  output_token_count: number;
  cache_token_count: number;
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

/** Aggregated token counts and estimated cost for one grouping. */
export type SessionUsageTotals = {
  request_count: number;
  input_tokens: number;
  output_tokens: number;
  cache_write_tokens: number;
  cache_read_tokens: number;
  /** Estimated cost in USD micros (1 USD = 1_000_000). */
  cost_micros: number;
  /**
   * Requests whose model has no known rate and therefore contribute no cost.
   * Non-zero means the total is a lower bound, not a complete figure.
   */
  unpriced_request_count: number;
};

/**
 * One provider or model row. The Rust side flattens `SessionUsageTotals` into
 * this object, so the totals fields appear inline rather than nested.
 */
export type SessionUsageRow = SessionUsageTotals & {
  /** `claude` or `codex`. */
  provider: string;
  /** Empty string on provider rollup rows. */
  model: string;
  priced: boolean;
};

/** Usage aggregated from local Claude Code and Codex CLI session transcripts. */
export type SessionUsageStats = {
  totals: SessionUsageTotals;
  by_provider: SessionUsageRow[];
  by_model: SessionUsageRow[];
  scanned_file_count: number;
  /** True when the file cap was hit, so the totals are incomplete. */
  truncated: boolean;
};

export type RouteProxyLiveLogEntry = {
  id: string;
  trace_id?: string | null;
  platform: string;
  credential_id: string;
  credential_name: string;
  attempt: number;
  path: string;
  target_url?: string | null;
  upstream_headers?: string | null;
  requested_model?: string | null;
  upstream_model?: string | null;
  status?: number | null;
  success: boolean;
  error_message?: string | null;
  duration_ms: number;
  bridge?: string | null;
  client_request?: string | null;
  upstream_request?: string | null;
  upstream_response?: string | null;
  final_response?: string | null;
  notes?: string[] | null;
  truncated: boolean;
  created_at: string;
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
  interface_format?: InterfaceFormat | null;
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
  tlsEnabled?: boolean;
  tlsCertPath?: string | null;
  tlsKeyPath?: string | null;
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

export type MobilePairingPayload = {
  v: 1;
  publicUrl?: string | null;
  privateUrl?: string | null;
  pairingCode: string;
  expiresAt: number;
};

export type MobilePairingRedeemResponse = {
  token: string;
  expiresAt: number;
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

export type RouteCredentialActivityEvent = {
  platform: string;
  credential_id: string;
  active_request_count: number;
  max_concurrency: number;
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

/** One client the platform can write config for, plus that file's current state. */
export type ConfigWriteClientStatus = {
  client_key: string;
  display_name: string;
  native: boolean;
  restart_required: boolean;
  target_key: string;
  platform: string;
  config_path?: string | null;
  file_status: string;
  error_code?: string | null;
};

export type AppSettings = {
  language: string;
  theme: string;
  copy_import_sources: boolean;
  logging_enabled: boolean;
  secret_storage: string;
  data_dir: string;
  ccswitch_deeplink_compat_enabled: boolean;
  /**
   * Pool-wide Claude Code client behavior switches as a JSON object string.
   * Claude Code reads these from its own settings file, which the whole pool
   * shares, so unlike model mappings they cannot be per-account.
   */
  claude_client_config_json?: string | null;
  /**
   * Which clients each platform writes config for, as
   * `{"codex":["codex","zcode"]}`. Recorded per platform because the dialog
   * always opens in one platform's context. Absent or empty means the
   * platform's native CLI only.
   */
  config_write_clients_json?: string | null;
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
  model?: string | null;
  reasoningEffort?: string | null;
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

export type AgentReasoningLevel = {
  effort: string;
  description: string;
};

export type AgentLaunchModel = {
  id: string;
  reasoningLevels: AgentReasoningLevel[];
  defaultReasoningLevel?: string | null;
};

export type AgentLaunchOption = {
  platform: string;
  displayName: string;
  program: string;
  installed: boolean;
  npmPackage: string;
  installCommand: string;
  supportsModelSelection: boolean;
  supportsReasoning: boolean;
  models: AgentLaunchModel[];
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

export type McpAppType =
  | "claude_code"
  | "codex"
  | "gemini"
  | "grok"
  | "open_claw"
  | "open_code"
  | "hermes"
  | "cline"
  | "cursor"
  | "kimi_code"
  | "code_buddy";

export type McpSpec = Record<string, unknown>;

export type LocalMcpServer = {
  id: string;
  spec: McpSpec;
  apps: McpAppType[];
};

export type McpMarketplaceProvider = {
  id: string;
  name: string;
  description: string;
};

export type McpMarketplaceItem = {
  provider_id: string;
  server_id: string;
  name: string;
  description: string;
  homepage?: string | null;
  remote: boolean;
  verified: boolean;
  icon_url?: string | null;
  latest_version?: string | null;
  protocols: string[];
  owner?: string | null;
  namespace?: string | null;
  downloads?: number | null;
  score?: number | null;
  is_deployed?: boolean | null;
};

export type McpMarketplaceInstallParameter = {
  key: string;
  label: string;
  description?: string | null;
  required: boolean;
  secret: boolean;
  kind: string;
  default_value?: unknown;
  placeholder?: string | null;
  enum_values: string[];
  location?: string | null;
};

export type McpMarketplaceInstallOption = {
  id: string;
  protocol: string;
  label: string;
  description?: string | null;
  spec: McpSpec;
  parameters: McpMarketplaceInstallParameter[];
};

export type McpMarketplaceServerDetail = McpMarketplaceItem & {
  default_option_id?: string | null;
  install_options: McpMarketplaceInstallOption[];
  spec: McpSpec;
};

export type SkillAgentType = McpAppType;
export type SkillScope = "global" | "project";
export type SkillLayout = "markdown_file" | "skill_directory";
export type SkillSource = "builtin" | "codex" | "agents" | "project" | "unknown";

export type SkillLocation = {
  scope: SkillScope;
  path: string;
  exists: boolean;
};

export type SkillItem = {
  id: string;
  name: string;
  scope: SkillScope;
  layout: SkillLayout;
  path: string;
  description?: string | null;
  read_only: boolean;
  package_id?: string | null;
  package_name?: string | null;
  category?: string | null;
  tags?: string[];
  language?: string | null;
  source?: SkillSource;
  version?: string | null;
  installed_at?: string | null;
  target_clients?: SkillAgentType[];
};

export type SkillsListResult = {
  supported: boolean;
  message?: string | null;
  locations: SkillLocation[];
  skills: SkillItem[];
};

export type SkillContent = {
  skill: SkillItem;
  content: string;
};

export type SkillAgentInfo = {
  agent_type: SkillAgentType;
  display_name: string;
  skills_capable: boolean;
};

export type SkillPackage = {
  id: string;
  name: string;
  description?: string | null;
  source: SkillSource;
  version?: string | null;
  manifest_path?: string | null;
  skill_ids: string[];
  skill_count: number;
  installed_skill_ids: string[];
  installed_count: number;
  installed_at?: string | null;
  read_only: boolean;
  target_clients: SkillAgentType[];
};

export type SkillPackageMember = {
  id: string;
  name: string;
  description?: string | null;
  category?: string | null;
  tags: string[];
  language?: string | null;
  installed: boolean;
  skill?: SkillItem | null;
};

export type SkillScanWarning = {
  code: string;
  path: string;
  message: string;
};

export type SkillsPackageListResult = {
  packages: SkillPackage[];
  skills: SkillItem[];
  warnings: SkillScanWarning[];
};

export type SkillPackageInstallResult = {
  package_id: string;
  installed_skill_ids: string[];
  skipped_skill_ids: string[];
};

export type SkillPackageDetail = {
  package: SkillPackage;
  skills: SkillItem[];
  members: SkillPackageMember[];
};
