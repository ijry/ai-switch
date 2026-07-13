CREATE TABLE IF NOT EXISTS sync_profiles (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  provider TEXT NOT NULL CHECK (provider IN ('local_folder', 'webdav', 's3', 'git')),
  endpoint_url TEXT,
  auth_ref TEXT,
  scope_json TEXT NOT NULL DEFAULT '{}',
  enabled INTEGER NOT NULL DEFAULT 1,
  notes TEXT,
  status TEXT NOT NULL DEFAULT 'configured',
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS sync_snapshots (
  id TEXT PRIMARY KEY,
  profile_id TEXT,
  direction TEXT NOT NULL CHECK (direction IN ('export', 'import')),
  status TEXT NOT NULL,
  item_counts_json TEXT NOT NULL DEFAULT '{}',
  manifest_json TEXT NOT NULL DEFAULT '{}',
  artifact_ref TEXT,
  created_at TEXT NOT NULL,
  FOREIGN KEY(profile_id) REFERENCES sync_profiles(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_sync_profiles_provider ON sync_profiles(provider);
CREATE INDEX IF NOT EXISTS idx_sync_profiles_enabled ON sync_profiles(enabled);
CREATE INDEX IF NOT EXISTS idx_sync_snapshots_profile ON sync_snapshots(profile_id);
CREATE INDEX IF NOT EXISTS idx_sync_snapshots_created_at ON sync_snapshots(created_at);
