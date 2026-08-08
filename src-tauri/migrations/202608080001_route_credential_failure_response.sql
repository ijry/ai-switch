PRAGMA foreign_keys = ON;

ALTER TABLE route_credentials ADD COLUMN last_failure_response_json TEXT;
