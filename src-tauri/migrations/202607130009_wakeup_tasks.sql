CREATE TABLE IF NOT EXISTS wakeup_tasks (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  managed_instance_id TEXT,
  target_app_id TEXT,
  provider_id TEXT,
  trigger_type TEXT NOT NULL CHECK (trigger_type IN ('manual', 'scheduled', 'interval')),
  schedule_json TEXT NOT NULL DEFAULT '{}',
  action_json TEXT NOT NULL DEFAULT '{}',
  enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
  status TEXT NOT NULL CHECK (status IN ('configured', 'paused', 'error')),
  last_run_at TEXT,
  notes TEXT,
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY(managed_instance_id) REFERENCES managed_instances(id) ON DELETE SET NULL,
  FOREIGN KEY(target_app_id) REFERENCES target_apps(id) ON DELETE SET NULL,
  FOREIGN KEY(provider_id) REFERENCES providers(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS wakeup_runs (
  id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL,
  outcome TEXT NOT NULL CHECK (outcome IN ('recorded', 'skipped', 'failed')),
  message TEXT NOT NULL,
  metadata_json TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL,
  FOREIGN KEY(task_id) REFERENCES wakeup_tasks(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_wakeup_tasks_enabled ON wakeup_tasks(enabled);
CREATE INDEX IF NOT EXISTS idx_wakeup_tasks_status ON wakeup_tasks(status);
CREATE INDEX IF NOT EXISTS idx_wakeup_tasks_instance ON wakeup_tasks(managed_instance_id);
CREATE INDEX IF NOT EXISTS idx_wakeup_runs_task ON wakeup_runs(task_id);
CREATE INDEX IF NOT EXISTS idx_wakeup_runs_created_at ON wakeup_runs(created_at);
