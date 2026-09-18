-- Stopwatch swaps team colours between halves. From here on, match_round and
-- match_event speak in stable teams: `Red` is the team whose overall colour in
-- the log is Red, even in a half where it wore Blue. This column records the
-- halves where it did.
--
-- Existing rows default to 0 and are corrected by the next `reprocess`, which
-- rebuilds every round from the stored raw logs.
ALTER TABLE match_round ADD COLUMN colours_swapped INTEGER NOT NULL DEFAULT 0;
