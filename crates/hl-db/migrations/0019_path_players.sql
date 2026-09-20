-- PLAN §14: routes are per player now, not only the owner's. A POV demo only
-- carries someone while its recorder can see them, so their routes break into
-- pieces; the recorder's own are whole. Rebuilt by the aim pass (version 7),
-- so the old rows go rather than being migrated.

DELETE FROM demo_path;
DELETE FROM aim_log;

ALTER TABLE demo_path ADD COLUMN account_id INTEGER NOT NULL DEFAULT 0;
