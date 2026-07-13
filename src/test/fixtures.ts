import type {
  AppSettings,
  BatchGroup,
  BulkOperation,
  FailoverPolicy,
  ItemTag,
  ManagedInstance,
  McpServer,
  OfficialAccount,
  OfficialAccountStatus,
  PromptAsset,
  Provider,
  PluginLink,
  ProviderPreset,
  ProxyProfile,
  QuotaSnapshot,
  SessionEvent,
  SessionRecord,
  SyncProfile,
  SyncSnapshot,
  TagRecord,
  TargetSwitchStatus,
  UpdateChannel,
  UpdateCheck,
  UsageEvent,
  WakeupRun,
  WakeupTask,
} from "../lib/api/types";

export const batchGroupsFixture: BatchGroup[] = [
  {
    batch: {
      id: "batch-1",
      name: "July imports",
      source: "example_json",
      notes: null,
      sort_order: 0,
      created_at: "2026-07-13T00:00:00Z",
      updated_at: "2026-07-13T00:00:00Z",
    },
    health: "warning",
    children: [
      {
        item_type: "provider",
        id: "provider-1",
        title: "Acme Claude",
        subtitle: "openai_compatible",
        status: "ok",
      },
      {
        item_type: "official_account",
        id: "account-1",
        title: "Team Account",
        subtitle: "team@example.com",
        status: "warning",
      },
    ],
  },
];

export const settingsFixture: AppSettings = {
  language: "zh-CN",
  theme: "system",
  copy_import_sources: false,
  logging_enabled: true,
  secret_storage: "keyring",
  data_dir: "C:/Users/example/.ai-switch",
};

export const providersFixture: Provider[] = [
  {
    id: "provider-1",
    name: "Acme Provider",
    kind: "openai_compatible",
    base_url: "https://api.example.com/v1",
    model_config_json: "{\"default\":\"gpt-4.1\"}",
    target_options_json: "{}",
    secret_ref: "secret://provider/acme",
    status: "ok",
    sort_order: 0,
    created_at: "2026-07-13T00:00:00Z",
    updated_at: "2026-07-13T00:00:00Z",
  },
];

export const officialAccountsFixture: OfficialAccount[] = [
  {
    id: "account-1",
    platform: "codex",
    display_name: "Team Codex",
    email: "team@example.com",
    plan: "team",
    account_metadata_json: "{\"workspace\":\"engineering\"}",
    secret_ref: "secret://account/team",
    quota_snapshot_id: null,
    status: "ok",
    sort_order: 0,
    created_at: "2026-07-13T00:00:00Z",
    updated_at: "2026-07-13T00:00:00Z",
  },
];

export const quotaSnapshotsFixture: QuotaSnapshot[] = [
  {
    id: "quota-1",
    owner_type: "official_account",
    owner_id: "account-1",
    status: "warning",
    remaining_label: "12% remaining",
    reset_at: "2026-07-14T00:00:00Z",
    summary_json: "{\"window\":\"daily\"}",
    raw_excerpt_json: "{}",
    fetched_at: "2026-07-13T01:00:00Z",
  },
];

export const officialAccountStatusesFixture: OfficialAccountStatus[] = [
  {
    account: {
      ...officialAccountsFixture[0],
      quota_snapshot_id: "quota-1",
    },
    quota_snapshot: quotaSnapshotsFixture[0],
  },
];

export const providerPresetsFixture: ProviderPreset[] = [
  {
    id: "openai-compatible",
    name: "OpenAI Compatible",
    description: "Generic OpenAI-compatible API using OPENAI_API_KEY.",
    kind: "openai_compatible",
    base_url: "https://api.openai.com/v1",
    model_config_json: "{\"default\":\"gpt-4.1\"}",
    target_options_json: "{\"env_key\":\"OPENAI_API_KEY\"}",
    secret_env_key: "OPENAI_API_KEY",
  },
];

export const targetSwitchStatusesFixture: TargetSwitchStatus[] = [
  {
    target: {
      id: "target-codex",
      key: "codex",
      display_name: "Codex",
      enabled: 1,
      sort_order: 2,
      created_at: "2026-07-13T00:00:00Z",
      updated_at: "2026-07-13T00:00:00Z",
    },
    active_provider: providersFixture[0],
    last_write_status: "written",
    last_error_code: null,
    last_written_at: "2026-07-13T00:00:00Z",
    last_snapshot_path: "C:/Users/example/.ai-switch/targets/codex/provider.json",
    last_snapshot_id: "snapshot-1",
    last_snapshot_operation: "switch_provider:sandbox",
    can_rollback: false,
  },
  {
    target: {
      id: "target-claude",
      key: "claude_code",
      display_name: "Claude Code",
      enabled: 1,
      sort_order: 0,
      created_at: "2026-07-13T00:00:00Z",
      updated_at: "2026-07-13T00:00:00Z",
    },
    active_provider: null,
    last_write_status: null,
    last_error_code: null,
    last_written_at: null,
    last_snapshot_path: null,
    last_snapshot_id: null,
    last_snapshot_operation: null,
    can_rollback: false,
  },
  {
    target: {
      id: "target-opencode",
      key: "opencode",
      display_name: "OpenCode",
      enabled: 1,
      sort_order: 4,
      created_at: "2026-07-13T00:00:00Z",
      updated_at: "2026-07-13T00:00:00Z",
    },
    active_provider: null,
    last_write_status: null,
    last_error_code: null,
    last_written_at: null,
    last_snapshot_path: null,
    last_snapshot_id: null,
    last_snapshot_operation: null,
    can_rollback: false,
  },
];

export const mcpServersFixture: McpServer[] = [
  {
    id: "mcp-1",
    name: "Filesystem",
    transport: "stdio",
    command: "npx",
    args_json: "[\"-y\",\"@modelcontextprotocol/server-filesystem\"]",
    url: null,
    env_json: "{\"FILESYSTEM_ROOT\":\"C:/Users/example/projects\"}",
    enabled: 1,
    notes: "Local project files",
    status: "configured",
    sort_order: 0,
    created_at: "2026-07-13T00:00:00Z",
    updated_at: "2026-07-13T00:00:00Z",
  },
  {
    id: "mcp-2",
    name: "Docs",
    transport: "sse",
    command: null,
    args_json: "[]",
    url: "https://mcp.example.com/sse",
    env_json: "{}",
    enabled: 0,
    notes: null,
    status: "configured",
    sort_order: 0,
    created_at: "2026-07-13T00:01:00Z",
    updated_at: "2026-07-13T00:01:00Z",
  },
];

export const promptAssetsFixture: PromptAsset[] = [
  {
    id: "prompt-1",
    item_type: "prompt",
    name: "Review Prompt",
    description: "Find risky behavior changes.",
    body: "Review this diff for regressions, missing tests, and unsafe assumptions.",
    tags_json: "[\"review\",\"quality\"]",
    metadata_json: "{\"owner\":\"engineering\"}",
    enabled: 1,
    status: "draft",
    sort_order: 0,
    created_at: "2026-07-13T00:00:00Z",
    updated_at: "2026-07-13T00:00:00Z",
  },
  {
    id: "skill-1",
    item_type: "skill",
    name: "Release Notes",
    description: null,
    body: "Summarize merged changes into user-facing release notes.",
    tags_json: "[\"release\"]",
    metadata_json: "{}",
    enabled: 0,
    status: "draft",
    sort_order: 0,
    created_at: "2026-07-13T00:01:00Z",
    updated_at: "2026-07-13T00:01:00Z",
  },
];

export const proxyProfilesFixture: ProxyProfile[] = [
  {
    id: "proxy-1",
    name: "Local Proxy",
    endpoint_url: "http://127.0.0.1:7890",
    auth_ref: "env://LOCAL_PROXY_AUTH",
    enabled: 1,
    notes: "Local proxy metadata",
    status: "configured",
    sort_order: 0,
    created_at: "2026-07-13T00:00:00Z",
    updated_at: "2026-07-13T00:00:00Z",
  },
];

export const failoverPoliciesFixture: FailoverPolicy[] = [
  {
    id: "failover-1",
    name: "Primary then backup",
    strategy: "ordered",
    provider_ids_json: "[\"provider-1\",\"provider-2\"]",
    enabled: 1,
    notes: "Prefer primary provider first.",
    status: "configured",
    sort_order: 0,
    created_at: "2026-07-13T00:00:00Z",
    updated_at: "2026-07-13T00:00:00Z",
  },
];

export const usageEventsFixture: UsageEvent[] = [
  {
    id: "usage-1",
    provider_id: "provider-1",
    official_account_id: null,
    source_label: "manual",
    metric_type: "request",
    amount: 12,
    unit: "count",
    metadata_json: "{\"window\":\"daily\"}",
    created_at: "2026-07-13T00:00:00Z",
  },
];

export const syncProfilesFixture: SyncProfile[] = [
  {
    id: "sync-1",
    name: "Team WebDAV",
    provider: "webdav",
    endpoint_url: "https://sync.example.com/ai-switch",
    auth_ref: "env://WEBDAV_TOKEN",
    scope_json: "{\"providers\":true}",
    enabled: 1,
    notes: "Shared export",
    status: "configured",
    sort_order: 0,
    created_at: "2026-07-13T00:00:00Z",
    updated_at: "2026-07-13T00:00:00Z",
  },
];

export const syncSnapshotsFixture: SyncSnapshot[] = [
  {
    id: "sync-snapshot-1",
    profile_id: "sync-1",
    direction: "export",
    status: "recorded",
    item_counts_json:
      "{\"providers\":1,\"official_accounts\":1,\"mcp_servers\":1,\"prompt_assets\":1,\"proxy_profiles\":1,\"failover_policies\":1,\"usage_events\":1}",
    manifest_json:
      "{\"schema\":\"ai-switch.sync.snapshot.v1\",\"direction\":\"export\",\"generated_at\":\"2026-07-13T00:00:00Z\"}",
    artifact_ref: null,
    created_at: "2026-07-13T00:00:00Z",
  },
];

export const sessionsFixture: SessionRecord[] = [
  {
    id: "session-1",
    title: "Release review",
    target_app_id: "target-codex",
    provider_id: "provider-1",
    official_account_id: "account-1",
    prompt_asset_id: "prompt-1",
    mcp_server_ids_json: "[\"mcp-1\"]",
    tags_json: "[\"review\"]",
    status: "draft",
    notes: "Prepare release notes",
    sort_order: 0,
    created_at: "2026-07-13T00:00:00Z",
    updated_at: "2026-07-13T00:00:00Z",
  },
];

export const sessionEventsFixture: SessionEvent[] = [
  {
    id: "session-event-1",
    session_id: "session-1",
    event_type: "note",
    message: "Started review",
    metadata_json: "{\"source\":\"manual\"}",
    created_at: "2026-07-13T00:00:00Z",
  },
];

export const updateChannelsFixture: UpdateChannel[] = [
  {
    id: "update-channel-1",
    name: "Stable",
    channel: "stable",
    feed_url: "https://updates.example.com/stable.json",
    enabled: 1,
    notes: "Main channel",
    status: "configured",
    sort_order: 0,
    created_at: "2026-07-13T00:00:00Z",
    updated_at: "2026-07-13T00:00:00Z",
  },
];

export const updateChecksFixture: UpdateCheck[] = [
  {
    id: "update-check-1",
    channel_id: "update-channel-1",
    current_version: "0.1.0",
    latest_version: "0.1.1",
    status: "available",
    release_notes_url: "https://updates.example.com/releases/0.1.1",
    details_json: "{\"source\":\"manual\"}",
    checked_at: "2026-07-13T00:00:00Z",
  },
];

export const managedInstancesFixture: ManagedInstance[] = [
  {
    id: "instance-1",
    name: "Codex Review",
    target_app_id: "target-codex",
    provider_id: "provider-1",
    launch_args_json: "[\"--profile\",\"review\"]",
    env_json: "{\"API_KEY\":\"env://API_KEY\"}",
    profile_json: "{\"workspace\":\"review\"}",
    status: "configured",
    notes: "Local metadata only",
    sort_order: 0,
    created_at: "2026-07-13T00:00:00Z",
    updated_at: "2026-07-13T00:00:00Z",
  },
];

export const wakeupTasksFixture: WakeupTask[] = [
  {
    id: "wakeup-task-1",
    name: "Morning review",
    managed_instance_id: "instance-1",
    target_app_id: "target-codex",
    provider_id: "provider-1",
    trigger_type: "manual",
    schedule_json: "{\"window\":\"morning\"}",
    action_json: "{\"kind\":\"status_record\"}",
    enabled: 1,
    status: "configured",
    last_run_at: null,
    notes: "Metadata only",
    sort_order: 0,
    created_at: "2026-07-13T00:00:00Z",
    updated_at: "2026-07-13T00:00:00Z",
  },
];

export const wakeupRunsFixture: WakeupRun[] = [
  {
    id: "wakeup-run-1",
    task_id: "wakeup-task-1",
    outcome: "recorded",
    message: "Ready for manual start",
    metadata_json: "{\"source\":\"manual\"}",
    created_at: "2026-07-13T00:00:00Z",
  },
];

export const tagsFixture: TagRecord[] = [
  {
    id: "tag-1",
    name: "review",
    color: "#3f6f5f",
    description: "Shared review items",
    sort_order: 0,
    created_at: "2026-07-13T00:00:00Z",
    updated_at: "2026-07-13T00:00:00Z",
  },
];

export const itemTagsFixture: ItemTag[] = [
  {
    id: "item-tag-1",
    tag_id: "tag-1",
    item_type: "provider",
    item_id: "provider-1",
    created_at: "2026-07-13T00:00:00Z",
  },
];

export const pluginLinksFixture: PluginLink[] = [
  {
    id: "plugin-link-1",
    name: "Review bridge",
    plugin_key: "review.bridge",
    item_type: "provider",
    item_id: "provider-1",
    config_json: "{\"mode\":\"metadata\"}",
    enabled: 1,
    status: "configured",
    notes: "Metadata only",
    sort_order: 0,
    created_at: "2026-07-13T00:00:00Z",
    updated_at: "2026-07-13T00:00:00Z",
  },
];

export const bulkOperationsFixture: BulkOperation[] = [
  {
    id: "bulk-operation-1",
    name: "Apply review tag",
    operation_type: "tag_apply",
    target_type: "provider",
    item_ids_json: "[\"provider-1\"]",
    parameters_json: "{\"tag_id\":\"tag-1\"}",
    dry_run: 1,
    status: "planned",
    summary_json: "{}",
    created_at: "2026-07-13T00:00:00Z",
    updated_at: "2026-07-13T00:00:00Z",
  },
];
