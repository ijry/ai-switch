-- The upstream response id is the join key between a proxied request and the CLI
-- transcript entry for the same request. Extracting it at write time avoids
-- re-scanning every metadata_json on each stats refresh.
--
-- NULL means unknown: a pre-migration row, a failed request that never got a
-- response, or a body preview that was truncated before the id.
ALTER TABLE usage_events ADD COLUMN upstream_response_id TEXT;

-- Rows are looked up by this id when merging, so the index carries the join.
CREATE INDEX IF NOT EXISTS idx_usage_events_upstream_response_id
  ON usage_events (upstream_response_id);
