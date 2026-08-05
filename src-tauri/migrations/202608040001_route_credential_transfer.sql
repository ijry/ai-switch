CREATE TABLE transfer_installation_identity (
  singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
  instance_id TEXT NOT NULL UNIQUE,
  created_at TEXT NOT NULL
);

CREATE TABLE route_credential_transfer_origins (
  route_credential_id TEXT PRIMARY KEY REFERENCES route_credentials(id) ON DELETE CASCADE,
  source_instance_id TEXT NOT NULL,
  source_credential_id TEXT NOT NULL,
  source_platform TEXT NOT NULL,
  source_kind TEXT NOT NULL,
  source_schema_version INTEGER NOT NULL,
  source_fingerprint TEXT NOT NULL,
  imported_at TEXT NOT NULL,
  UNIQUE(source_instance_id, source_credential_id, source_platform, source_kind)
);

CREATE INDEX idx_transfer_origins_fingerprint
  ON route_credential_transfer_origins(source_fingerprint);
