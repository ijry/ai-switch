PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS route_credential_models (
  route_credential_id TEXT NOT NULL,
  model_key TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'ok' CHECK (status IN ('ok', 'error', 'paused')),
  transient_failure_count INTEGER NOT NULL DEFAULT 0,
  cooldown_until TEXT,
  semantic_failure_streak_count INTEGER NOT NULL DEFAULT 0,
  semantic_failure_streak_fingerprint TEXT,
  last_failure_kind TEXT,
  last_failure_message TEXT,
  last_failure_response_json TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (route_credential_id, model_key),
  FOREIGN KEY (route_credential_id) REFERENCES route_credentials(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_route_credential_models_lookup
  ON route_credential_models(route_credential_id, status, cooldown_until);
