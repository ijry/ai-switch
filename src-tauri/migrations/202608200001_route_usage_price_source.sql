-- Distinguish an amount the upstream actually returned from one this app
-- estimated locally from token counts and the model price table.
--
-- NULL means unknown (pre-migration rows, or a request with no price at all).
-- 'upstream' means the response carried an explicit price; 'estimated' means the
-- amount was computed from tokens and may not match the real bill.
ALTER TABLE usage_events ADD COLUMN price_source TEXT;

-- Backfill: every priced row that exists today came from an upstream price,
-- because local estimation did not exist before this migration.
UPDATE usage_events
SET price_source = 'upstream'
WHERE price_source IS NULL
  AND (price_usd_micros IS NOT NULL OR price_cny_micros IS NOT NULL);
