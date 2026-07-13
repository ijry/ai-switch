CREATE TABLE IF NOT EXISTS managed_instances (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  target_app_id TEXT,
  provider_id TEXT,
  launch_args_json TEXT NOT NULL DEFAULT '[]',
  env_json TEXT NOT NULL DEFAULT '{}',
  profile_json TEXT NOT NULL DEFAULT '{}',
  status TEXT NOT NULL CHECK (status IN ('configured', 'running', 'stopped', 'error')),
  notes TEXT,
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY(target_app_id) REFERENCES target_apps(id) ON DELETE SET NULL,
  FOREIGN KEY(provider_id) REFERENCES providers(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_managed_instances_status ON managed_instances(status);
CREATE INDEX IF NOT EXISTS idx_managed_instances_target ON managed_instances(target_app_id);
CREATE INDEX IF NOT EXISTS idx_managed_instances_provider ON managed_instances(provider_id);
