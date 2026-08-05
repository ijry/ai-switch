ALTER TABLE route_credentials ADD COLUMN archived_at TEXT;

CREATE INDEX IF NOT EXISTS idx_route_credentials_archive
  ON route_credentials(platform, archived_at, sort_order);
