-- Per-platform model catalog mode for the route pool.
--
-- Lives in the database rather than settings.json because the proxy resolves
-- `/v1/models` with nothing but a SqlitePool in hand.
CREATE TABLE IF NOT EXISTS route_pool_model_modes (
  platform TEXT PRIMARY KEY,
  mode TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
