PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS route_proxy_key_aliases (
  proxy_key TEXT PRIMARY KEY,
  platform TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_route_proxy_key_aliases_platform
  ON route_proxy_key_aliases(platform);
