CREATE TABLE IF NOT EXISTS tags (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL UNIQUE,
  color TEXT,
  description TEXT,
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS item_tags (
  id TEXT PRIMARY KEY,
  tag_id TEXT NOT NULL,
  item_type TEXT NOT NULL CHECK (item_type IN ('provider', 'official_account', 'mcp_server', 'prompt_asset', 'session', 'managed_instance', 'wakeup_task', 'target_app', 'mixed')),
  item_id TEXT NOT NULL,
  created_at TEXT NOT NULL,
  FOREIGN KEY(tag_id) REFERENCES tags(id) ON DELETE CASCADE,
  UNIQUE(tag_id, item_type, item_id)
);

CREATE TABLE IF NOT EXISTS plugin_links (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  plugin_key TEXT NOT NULL,
  item_type TEXT NOT NULL CHECK (item_type IN ('provider', 'official_account', 'mcp_server', 'prompt_asset', 'session', 'managed_instance', 'wakeup_task', 'target_app', 'mixed')),
  item_id TEXT NOT NULL,
  config_json TEXT NOT NULL DEFAULT '{}',
  enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
  status TEXT NOT NULL CHECK (status IN ('configured', 'paused', 'error')),
  notes TEXT,
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS bulk_operations (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  operation_type TEXT NOT NULL CHECK (operation_type IN ('tag_apply', 'tag_remove', 'status_record', 'export_selection', 'plugin_link')),
  target_type TEXT NOT NULL CHECK (target_type IN ('provider', 'official_account', 'mcp_server', 'prompt_asset', 'session', 'managed_instance', 'wakeup_task', 'target_app', 'mixed')),
  item_ids_json TEXT NOT NULL DEFAULT '[]',
  parameters_json TEXT NOT NULL DEFAULT '{}',
  dry_run INTEGER NOT NULL DEFAULT 1 CHECK (dry_run IN (0, 1)),
  status TEXT NOT NULL CHECK (status IN ('planned', 'recorded', 'cancelled', 'error')),
  summary_json TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_tags_name ON tags(name);
CREATE INDEX IF NOT EXISTS idx_item_tags_item ON item_tags(item_type, item_id);
CREATE INDEX IF NOT EXISTS idx_item_tags_tag ON item_tags(tag_id);
CREATE INDEX IF NOT EXISTS idx_plugin_links_item ON plugin_links(item_type, item_id);
CREATE INDEX IF NOT EXISTS idx_plugin_links_plugin ON plugin_links(plugin_key);
CREATE INDEX IF NOT EXISTS idx_bulk_operations_status ON bulk_operations(status);
CREATE INDEX IF NOT EXISTS idx_bulk_operations_type ON bulk_operations(operation_type);
