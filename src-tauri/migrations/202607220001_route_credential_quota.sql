PRAGMA foreign_keys = ON;

-- SQL-level official quota fields for pool selection and list display.
ALTER TABLE route_credentials ADD COLUMN subscription_type TEXT;
ALTER TABLE route_credentials ADD COLUMN quota_remaining INTEGER;
ALTER TABLE route_credentials ADD COLUMN quota_limit INTEGER;
ALTER TABLE route_credentials ADD COLUMN quota_used INTEGER;
ALTER TABLE route_credentials ADD COLUMN quota_updated_at TEXT;

-- Backfill from existing config_json snapshots when present.
UPDATE route_credentials
SET
  subscription_type = NULLIF(TRIM(json_extract(config_json, '$.subscription_type')), ''),
  quota_remaining = CASE
    WHEN json_extract(config_json, '$.quota_remaining') IS NULL THEN NULL
    ELSE CAST(json_extract(config_json, '$.quota_remaining') AS INTEGER)
  END,
  quota_limit = CASE
    WHEN json_extract(config_json, '$.quota_limit') IS NULL THEN NULL
    ELSE CAST(json_extract(config_json, '$.quota_limit') AS INTEGER)
  END,
  quota_used = CASE
    WHEN json_extract(config_json, '$.quota_used') IS NULL THEN NULL
    ELSE CAST(json_extract(config_json, '$.quota_used') AS INTEGER)
  END,
  quota_updated_at = NULLIF(TRIM(json_extract(config_json, '$.quota_updated_at')), '');

CREATE INDEX IF NOT EXISTS idx_route_credentials_pool_quota
  ON route_credentials(platform, status, quota_remaining);
