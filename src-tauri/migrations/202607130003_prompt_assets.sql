CREATE TABLE IF NOT EXISTS prompt_assets (
  id TEXT PRIMARY KEY,
  item_type TEXT NOT NULL CHECK (item_type IN ('prompt', 'skill')),
  name TEXT NOT NULL,
  description TEXT,
  body TEXT NOT NULL,
  tags_json TEXT NOT NULL DEFAULT '[]',
  metadata_json TEXT NOT NULL DEFAULT '{}',
  enabled INTEGER NOT NULL DEFAULT 1,
  status TEXT NOT NULL DEFAULT 'draft',
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_prompt_assets_type ON prompt_assets(item_type);
CREATE INDEX IF NOT EXISTS idx_prompt_assets_enabled ON prompt_assets(enabled);
CREATE INDEX IF NOT EXISTS idx_prompt_assets_name ON prompt_assets(name);
