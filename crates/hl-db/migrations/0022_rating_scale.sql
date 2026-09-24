-- PLAN: the rating becomes HLTV-shaped — 1.00 is an average game rather than
-- 50 out of 100.
--
-- The conversion needs the middle of the pool and its spread, and both have to
-- outlive the pass that measured them: a game rated on its own as it arrives
-- must land on the same scale as the games it is listed beside.
--
-- One row per model version. DERIVED by the full rating pass.

CREATE TABLE rating_scale (
    model_version TEXT PRIMARY KEY,
    mean          REAL NOT NULL,     -- of the weighted percentile, 0-100
    sd            REAL NOT NULL,
    n             INTEGER NOT NULL,  -- performances it was measured from
    made_at       TEXT NOT NULL DEFAULT (datetime('now'))
) STRICT;
