CREATE TABLE IF NOT EXISTS update_channels (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  channel TEXT NOT NULL CHECK (channel IN ('stable', 'beta', 'nightly')),
  feed_url TEXT,
  enabled INTEGER NOT NULL DEFAULT 1,
  notes TEXT,
  status TEXT NOT NULL DEFAULT 'configured',
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS update_checks (
  id TEXT PRIMARY KEY,
  channel_id TEXT,
  current_version TEXT NOT NULL,
  latest_version TEXT,
  status TEXT NOT NULL CHECK (status IN ('unknown', 'up_to_date', 'available', 'error')),
  release_notes_url TEXT,
  details_json TEXT NOT NULL DEFAULT '{}',
  checked_at TEXT NOT NULL,
  FOREIGN KEY(channel_id) REFERENCES update_channels(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_update_channels_channel ON update_channels(channel);
CREATE INDEX IF NOT EXISTS idx_update_channels_enabled ON update_channels(enabled);
CREATE INDEX IF NOT EXISTS idx_update_checks_channel ON update_checks(channel_id);
CREATE INDEX IF NOT EXISTS idx_update_checks_checked_at ON update_checks(checked_at);
