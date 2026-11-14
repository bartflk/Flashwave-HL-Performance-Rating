-- The 6v6 league sample's raw server logs (the 6v6 plan, P6): who was alive
-- at each kill, for 6v6's own round-win table and each kill's round impact.
-- SOURCE data from drops.tf.
--   raw        the zipped server log
--   raw_state  NULL not tried yet, 'ok', or 'missing'
ALTER TABLE sixes_league_log ADD COLUMN raw BLOB;
ALTER TABLE sixes_league_log ADD COLUMN raw_state TEXT;
