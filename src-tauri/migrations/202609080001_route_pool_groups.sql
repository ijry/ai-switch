-- Core dynamic route groups shared by the agent UI, ordinary routing, and SaaS.
CREATE TABLE IF NOT EXISTS route_pool_groups (
  id TEXT PRIMARY KEY,
  platform TEXT NOT NULL,
  name TEXT NOT NULL,
  sort_order INTEGER NOT NULL DEFAULT 0,
  is_internal INTEGER NOT NULL DEFAULT 0,
  is_active INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  deleted_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_route_pool_groups_platform
  ON route_pool_groups(platform, deleted_at, sort_order);
CREATE UNIQUE INDEX IF NOT EXISTS idx_route_pool_groups_one_active
  ON route_pool_groups(platform) WHERE is_active = 1 AND deleted_at IS NULL;

ALTER TABLE route_pool_members ADD COLUMN group_id TEXT;
CREATE INDEX IF NOT EXISTS idx_route_pool_members_group
  ON route_pool_members(group_id);

WITH seed(platform, slug, name, sort_order, is_active) AS (
  VALUES
    ('codex', 'default', '默认组', 0, 1),
    ('codex', 'out', '未入池', 1, 0),
    ('codex', 'archived', '已归档', 2, 0),
    ('claude', 'default', '默认组', 0, 1),
    ('claude', 'out', '未入池', 1, 0),
    ('claude', 'archived', '已归档', 2, 0),
    ('gemini', 'default', '默认组', 0, 1),
    ('gemini', 'out', '未入池', 1, 0),
    ('gemini', 'archived', '已归档', 2, 0),
    ('grok', 'default', '默认组', 0, 1),
    ('grok', 'out', '未入池', 1, 0),
    ('grok', 'archived', '已归档', 2, 0),
    ('opencode', 'default', '默认组', 0, 1),
    ('opencode', 'out', '未入池', 1, 0),
    ('opencode', 'archived', '已归档', 2, 0),
    ('openclaw', 'default', '默认组', 0, 1),
    ('openclaw', 'out', '未入池', 1, 0),
    ('openclaw', 'archived', '已归档', 2, 0),
    ('hermes', 'default', '默认组', 0, 1),
    ('hermes', 'out', '未入池', 1, 0),
    ('hermes', 'archived', '已归档', 2, 0)
)
INSERT INTO route_pool_groups
  (id, platform, name, sort_order, is_internal, is_active, created_at, updated_at)
SELECT
  printf('%s-%s', platform, slug),
  platform,
  name,
  sort_order,
  0,
  is_active,
  '2026-09-08T00:00:00Z',
  '2026-09-08T00:00:00Z'
FROM seed
WHERE NOT EXISTS (
  SELECT 1 FROM route_pool_groups existing WHERE existing.id = printf('%s-%s', seed.platform, seed.slug)
);
