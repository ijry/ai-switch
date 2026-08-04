ALTER TABLE target_apps ADD COLUMN platform TEXT;

UPDATE target_apps
SET platform = CASE key
  WHEN 'claude_code' THEN 'claude'
  WHEN 'claude_desktop' THEN 'claude'
  WHEN 'codex' THEN 'codex'
  WHEN 'gemini_cli' THEN 'gemini'
  WHEN 'grok' THEN 'grok'
  WHEN 'opencode' THEN 'opencode'
  WHEN 'openclaw' THEN 'openclaw'
  WHEN 'hermes' THEN 'hermes'
  ELSE NULL
END;

ALTER TABLE config_snapshots ADD COLUMN platform TEXT;
ALTER TABLE config_snapshots ADD COLUMN operation_group_id TEXT;
ALTER TABLE config_snapshots ADD COLUMN source_snapshot_id TEXT;
ALTER TABLE config_snapshots ADD COLUMN original_file_existed INTEGER NOT NULL DEFAULT 0 CHECK (original_file_existed IN (0, 1));
ALTER TABLE config_snapshots ADD COLUMN metadata_json TEXT NOT NULL DEFAULT '{}';
ALTER TABLE config_snapshots ADD COLUMN updated_at TEXT NOT NULL DEFAULT '';

UPDATE config_snapshots SET updated_at = created_at WHERE updated_at = '';

CREATE INDEX IF NOT EXISTS idx_config_snapshots_target_created
  ON config_snapshots(target_app_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_config_snapshots_group
  ON config_snapshots(operation_group_id);
