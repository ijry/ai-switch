ALTER TABLE saas_group_models ADD COLUMN image_price_micros INTEGER NOT NULL DEFAULT 0 CHECK(image_price_micros>=0);
