PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS target_apps (
  id TEXT PRIMARY KEY,
  key TEXT NOT NULL UNIQUE,
  display_name TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 1,
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS providers (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  kind TEXT NOT NULL,
  base_url TEXT,
  model_config_json TEXT NOT NULL DEFAULT '{}',
  target_options_json TEXT NOT NULL DEFAULT '{}',
  secret_ref TEXT,
  status TEXT NOT NULL DEFAULT 'ok',
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS official_accounts (
  id TEXT PRIMARY KEY,
  platform TEXT NOT NULL,
  display_name TEXT NOT NULL,
  email TEXT,
  plan TEXT,
  account_metadata_json TEXT NOT NULL DEFAULT '{}',
  secret_ref TEXT,
  quota_snapshot_id TEXT,
  status TEXT NOT NULL DEFAULT 'ok',
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS batches (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  source TEXT NOT NULL DEFAULT 'manual',
  notes TEXT,
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS batch_items (
  id TEXT PRIMARY KEY,
  batch_id TEXT NOT NULL,
  item_type TEXT NOT NULL CHECK (item_type IN ('provider', 'official_account')),
  item_id TEXT NOT NULL,
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  UNIQUE(batch_id, item_type, item_id),
  FOREIGN KEY(batch_id) REFERENCES batches(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS import_jobs (
  id TEXT PRIMARY KEY,
  source_type TEXT NOT NULL,
  source_label TEXT NOT NULL,
  batch_id TEXT,
  strategy TEXT NOT NULL,
  status TEXT NOT NULL,
  success_count INTEGER NOT NULL DEFAULT 0,
  failure_count INTEGER NOT NULL DEFAULT 0,
  conflict_count INTEGER NOT NULL DEFAULT 0,
  summary_json TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL,
  completed_at TEXT,
  FOREIGN KEY(batch_id) REFERENCES batches(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS target_app_states (
  id TEXT PRIMARY KEY,
  target_app_id TEXT NOT NULL UNIQUE,
  active_item_type TEXT,
  active_item_id TEXT,
  last_write_status TEXT,
  last_error_code TEXT,
  last_written_at TEXT,
  updated_at TEXT NOT NULL,
  FOREIGN KEY(target_app_id) REFERENCES target_apps(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS config_snapshots (
  id TEXT PRIMARY KEY,
  target_app_id TEXT,
  operation TEXT NOT NULL,
  path TEXT NOT NULL,
  before_hash TEXT,
  after_hash TEXT,
  backup_path TEXT,
  status TEXT NOT NULL,
  error_code TEXT,
  created_at TEXT NOT NULL,
  FOREIGN KEY(target_app_id) REFERENCES target_apps(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS quota_snapshots (
  id TEXT PRIMARY KEY,
  owner_type TEXT NOT NULL,
  owner_id TEXT NOT NULL,
  status TEXT NOT NULL,
  remaining_label TEXT,
  reset_at TEXT,
  summary_json TEXT NOT NULL DEFAULT '{}',
  raw_excerpt_json TEXT NOT NULL DEFAULT '{}',
  fetched_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS secure_secrets (
  id TEXT PRIMARY KEY,
  provider TEXT NOT NULL,
  external_ref TEXT NOT NULL,
  label TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_batch_items_batch_id ON batch_items(batch_id);
CREATE INDEX IF NOT EXISTS idx_batch_items_item ON batch_items(item_type, item_id);
CREATE INDEX IF NOT EXISTS idx_providers_name ON providers(name);
CREATE INDEX IF NOT EXISTS idx_accounts_platform ON official_accounts(platform);
CREATE INDEX IF NOT EXISTS idx_import_jobs_created_at ON import_jobs(created_at);
