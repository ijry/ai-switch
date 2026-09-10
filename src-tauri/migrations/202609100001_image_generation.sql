PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS imagegen_sessions (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  platform TEXT NOT NULL CHECK (platform IN ('codex', 'gemini')),
  archived INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_imagegen_sessions_recent
  ON imagegen_sessions(archived, updated_at DESC);

CREATE TABLE IF NOT EXISTS imagegen_messages (
  id TEXT PRIMARY KEY,
  session_id TEXT NOT NULL REFERENCES imagegen_sessions(id) ON DELETE CASCADE,
  role TEXT NOT NULL CHECK (role IN ('user', 'assistant')),
  prompt TEXT NOT NULL DEFAULT '',
  request_json TEXT NOT NULL DEFAULT '{}',
  status TEXT NOT NULL,
  model TEXT,
  upstream_model TEXT,
  member_id TEXT,
  upstream_response_id TEXT,
  error_message TEXT,
  image_count INTEGER NOT NULL DEFAULT 0,
  cost_micros INTEGER,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_imagegen_messages_session
  ON imagegen_messages(session_id, created_at ASC);

CREATE TABLE IF NOT EXISTS imagegen_assets (
  id TEXT PRIMARY KEY,
  session_id TEXT NOT NULL REFERENCES imagegen_sessions(id) ON DELETE CASCADE,
  message_id TEXT NOT NULL REFERENCES imagegen_messages(id) ON DELETE CASCADE,
  relative_path TEXT NOT NULL,
  mime_type TEXT NOT NULL,
  sha256 TEXT NOT NULL,
  width INTEGER,
  height INTEGER,
  byte_size INTEGER NOT NULL,
  created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_imagegen_assets_message
  ON imagegen_assets(message_id, created_at ASC);
