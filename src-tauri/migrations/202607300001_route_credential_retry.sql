PRAGMA foreign_keys = ON;

ALTER TABLE route_credentials ADD COLUMN transient_failure_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE route_credentials ADD COLUMN next_retry_at TEXT;
ALTER TABLE route_credentials ADD COLUMN cooldown_until TEXT;
ALTER TABLE route_credentials ADD COLUMN last_failure_kind TEXT;
ALTER TABLE route_credentials ADD COLUMN last_failure_message TEXT;

CREATE INDEX IF NOT EXISTS idx_route_credentials_retry
  ON route_credentials(platform, status, next_retry_at, cooldown_until);
