import type { AppSettings, BatchGroup } from "../lib/api/types";

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
        platform: null,
        status: "ok",
      },
      {
        item_type: "official_account",
        id: "account-1",
        title: "Team Account",
        subtitle: "team@example.com",
        platform: "codex",
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
