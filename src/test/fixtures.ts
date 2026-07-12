import type {
  AppSettings,
  BatchGroup,
  Provider,
  TargetSwitchStatus,
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
    health: "ok",
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
        status: "ok",
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
  },
];
