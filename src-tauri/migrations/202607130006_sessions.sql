CREATE TABLE IF NOT EXISTS sessions (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  target_app_id TEXT,
  provider_id TEXT,
  official_account_id TEXT,
  prompt_asset_id TEXT,
  mcp_server_ids_json TEXT NOT NULL DEFAULT '[]',
  tags_json TEXT NOT NULL DEFAULT '[]',
  status TEXT NOT NULL CHECK (status IN ('draft', 'active', 'archived')),
  notes TEXT,
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY(target_app_id) REFERENCES target_apps(id) ON DELETE SET NULL,
  FOREIGN KEY(provider_id) REFERENCES providers(id) ON DELETE SET NULL,
  FOREIGN KEY(official_account_id) REFERENCES official_accounts(id) ON DELETE SET NULL,
  FOREIGN KEY(prompt_asset_id) REFERENCES prompt_assets(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS session_events (
  id TEXT PRIMARY KEY,
  session_id TEXT NOT NULL,
  event_type TEXT NOT NULL CHECK (event_type IN ('note', 'status', 'usage', 'quota', 'error', 'import', 'switch')),
  message TEXT NOT NULL,
  metadata_json TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL,
  FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_sessions_status ON sessions(status);
CREATE INDEX IF NOT EXISTS idx_sessions_provider ON sessions(provider_id);
CREATE INDEX IF NOT EXISTS idx_session_events_session ON session_events(session_id);
CREATE INDEX IF NOT EXISTS idx_session_events_created_at ON session_events(created_at);
