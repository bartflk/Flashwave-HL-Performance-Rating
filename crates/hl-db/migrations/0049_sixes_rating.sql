-- The 6v6 rating's preview (the 6v6 plan, P6): one row a player, log and
-- role -- a main role (pocket, roamer, flank_scout, pocket_scout, demoman,
-- medic) or an off-class (off:engineer, ...) -- kept apart from Highlander's
-- `rating`, whose key has no role and whose readers are Highlander's.
-- DERIVED from the stored 6v6 logs; rebuilt whole by each rating pass.
--   rating   on the app's scale: 1.00 average, 0.25 a standard deviation
--   score    the weighted average percentile, 0-100
--   parts    JSON: [component, percentile, share] for the breakdown
CREATE TABLE sixes_rating (
    log_id        INTEGER NOT NULL,
    account_id    INTEGER NOT NULL,
    role          TEXT    NOT NULL,
    model_version TEXT    NOT NULL,
    rating        REAL    NOT NULL,
    score         REAL    NOT NULL,
    minutes       REAL    NOT NULL,
    parts         TEXT    NOT NULL,
    PRIMARY KEY (log_id, account_id, role, model_version)
) STRICT;

CREATE INDEX sixes_rating_account ON sixes_rating (account_id, model_version, role);
