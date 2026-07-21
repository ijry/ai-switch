PRAGMA foreign_keys = ON;

-- Unified official quota windows for Codex/Claude/Grok pool selection.
ALTER TABLE route_credentials ADD COLUMN primary_remain INTEGER;
ALTER TABLE route_credentials ADD COLUMN weekly_remain INTEGER;
ALTER TABLE route_credentials ADD COLUMN reset_primary TEXT;
ALTER TABLE route_credentials ADD COLUMN reset_weekly TEXT;

-- Backfill from previous single remaining column / config_json.
UPDATE route_credentials
SET
  primary_remain = COALESCE(
    CASE
      WHEN json_extract(config_json, '$.primary_remain') IS NULL THEN NULL
      ELSE CAST(json_extract(config_json, '$.primary_remain') AS INTEGER)
    END,
    quota_remaining,
    CASE
      WHEN json_extract(config_json, '$.quota_remaining') IS NULL THEN NULL
      ELSE CAST(json_extract(config_json, '$.quota_remaining') AS INTEGER)
    END
  ),
  weekly_remain = CASE
    WHEN json_extract(config_json, '$.weekly_remain') IS NULL THEN NULL
    ELSE CAST(json_extract(config_json, '$.weekly_remain') AS INTEGER)
  END,
  reset_primary = COALESCE(
    NULLIF(TRIM(json_extract(config_json, '$.reset_primary')), ''),
    NULLIF(TRIM(json_extract(config_json, '$.quota_updated_at')), '')
  ),
  reset_weekly = NULLIF(TRIM(json_extract(config_json, '$.reset_weekly')), '');

CREATE INDEX IF NOT EXISTS idx_route_credentials_pool_quota_windows
  ON route_credentials(platform, status, primary_remain, weekly_remain);
