PRAGMA foreign_keys = ON;

ALTER TABLE route_credentials ADD COLUMN semantic_failure_streak_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE route_credentials ADD COLUMN semantic_failure_streak_fingerprint TEXT;
