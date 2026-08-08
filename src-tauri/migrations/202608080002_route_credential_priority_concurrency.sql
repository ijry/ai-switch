PRAGMA foreign_keys = ON;

ALTER TABLE route_credentials
  ADD COLUMN route_priority INTEGER NOT NULL DEFAULT 3
  CHECK (route_priority BETWEEN 1 AND 5);

ALTER TABLE route_credentials
  ADD COLUMN max_concurrency INTEGER NOT NULL DEFAULT 1
  CHECK (max_concurrency >= 1);

CREATE INDEX IF NOT EXISTS idx_route_credentials_routing_priority
  ON route_credentials(platform, route_priority, status, next_retry_at, cooldown_until);
