CREATE TABLE IF NOT EXISTS proxy_profiles (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  endpoint_url TEXT NOT NULL,
  auth_ref TEXT,
  enabled INTEGER NOT NULL DEFAULT 1,
  notes TEXT,
  status TEXT NOT NULL DEFAULT 'configured',
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS failover_policies (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  strategy TEXT NOT NULL CHECK (strategy IN ('ordered', 'round_robin')),
  provider_ids_json TEXT NOT NULL DEFAULT '[]',
  enabled INTEGER NOT NULL DEFAULT 1,
  notes TEXT,
  status TEXT NOT NULL DEFAULT 'configured',
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS usage_events (
  id TEXT PRIMARY KEY,
  provider_id TEXT,
  official_account_id TEXT,
  source_label TEXT NOT NULL DEFAULT 'manual',
  metric_type TEXT NOT NULL,
  amount INTEGER NOT NULL DEFAULT 0,
  unit TEXT NOT NULL,
  metadata_json TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL,
  FOREIGN KEY(provider_id) REFERENCES providers(id) ON DELETE SET NULL,
  FOREIGN KEY(official_account_id) REFERENCES official_accounts(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS route_pool_members (
  id TEXT PRIMARY KEY,
  platform TEXT NOT NULL,
  official_account_id TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 1,
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE(platform, official_account_id),
  FOREIGN KEY(official_account_id) REFERENCES official_accounts(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS route_pool_cursors (
  platform TEXT PRIMARY KEY,
  next_index INTEGER NOT NULL DEFAULT 0,
  updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_proxy_profiles_enabled ON proxy_profiles(enabled);
CREATE INDEX IF NOT EXISTS idx_failover_policies_enabled ON failover_policies(enabled);
CREATE INDEX IF NOT EXISTS idx_usage_events_provider ON usage_events(provider_id);
CREATE INDEX IF NOT EXISTS idx_usage_events_account ON usage_events(official_account_id);
CREATE INDEX IF NOT EXISTS idx_usage_events_created_at ON usage_events(created_at);
CREATE INDEX IF NOT EXISTS idx_route_pool_members_platform ON route_pool_members(platform, enabled);
CREATE INDEX IF NOT EXISTS idx_route_pool_members_account ON route_pool_members(official_account_id);
