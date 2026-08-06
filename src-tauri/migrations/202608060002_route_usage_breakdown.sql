ALTER TABLE usage_events ADD COLUMN input_tokens INTEGER;
ALTER TABLE usage_events ADD COLUMN output_tokens INTEGER;
ALTER TABLE usage_events ADD COLUMN cache_tokens INTEGER;
ALTER TABLE usage_events ADD COLUMN price_usd_micros INTEGER;
ALTER TABLE usage_events ADD COLUMN price_cny_micros INTEGER;
ALTER TABLE usage_events ADD COLUMN price_currency TEXT;
