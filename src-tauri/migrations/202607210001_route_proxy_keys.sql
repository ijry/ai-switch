PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS route_proxy_keys (
  platform TEXT PRIMARY KEY,
  proxy_key TEXT NOT NULL UNIQUE,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_route_proxy_keys_proxy_key
  ON route_proxy_keys(proxy_key);
