PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS route_credentials (
  id TEXT PRIMARY KEY,
  platform TEXT NOT NULL,
  kind TEXT NOT NULL CHECK (kind IN ('official', 'api')),
  display_name TEXT NOT NULL,
  email TEXT,
  status TEXT NOT NULL DEFAULT 'ok',
  sort_order INTEGER NOT NULL DEFAULT 0,
  batch_id TEXT,
  secret_payload_json TEXT NOT NULL DEFAULT '{}',
  config_json TEXT NOT NULL DEFAULT '{}',
  preview_json TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY(batch_id) REFERENCES batches(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_route_credentials_platform
  ON route_credentials(platform, kind, status);
CREATE INDEX IF NOT EXISTS idx_route_credentials_batch
  ON route_credentials(batch_id);

-- Rebuild pool members around credentials. No legacy data migration.
DROP TABLE IF EXISTS route_pool_members;
CREATE TABLE route_pool_members (
  id TEXT PRIMARY KEY,
  platform TEXT NOT NULL,
  route_credential_id TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 1,
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE(platform, route_credential_id),
  FOREIGN KEY(route_credential_id) REFERENCES route_credentials(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_route_pool_members_platform
  ON route_pool_members(platform, enabled);
CREATE INDEX IF NOT EXISTS idx_route_pool_members_credential
  ON route_pool_members(route_credential_id);

-- Additive usage column for credential-scoped stats.
ALTER TABLE usage_events ADD COLUMN route_credential_id TEXT;
CREATE INDEX IF NOT EXISTS idx_usage_events_route_credential
  ON usage_events(route_credential_id);
