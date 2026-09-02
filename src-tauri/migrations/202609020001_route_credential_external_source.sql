PRAGMA foreign_keys = ON;

-- Records which third-party client an account was imported from, so a repeated
-- import overwrites the same row instead of creating a duplicate. The pair is
-- the source's own primary key: `external_source_client` names the tool and
-- `external_source_id` is that tool's record id.
ALTER TABLE route_credentials ADD COLUMN external_source_client TEXT;
ALTER TABLE route_credentials ADD COLUMN external_source_id TEXT;

-- Partial index: accounts created by hand leave both columns NULL and must not
-- collide with each other.
CREATE UNIQUE INDEX IF NOT EXISTS idx_route_credentials_external_source
  ON route_credentials(external_source_client, external_source_id)
  WHERE external_source_client IS NOT NULL AND external_source_id IS NOT NULL;
