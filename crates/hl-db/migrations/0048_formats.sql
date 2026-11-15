-- 6v6 beside Highlander (the 6v6 plan, P0). The app is growing to cover all
-- of competitive TF2; formats are never mixed, so league data says which
-- format it is. Everything stored before this is Highlander's.

-- An ETF2L competition's format: 'highlander' or 'sixes'. ETF2L's ids are
-- shared across formats, so a 6v6 competition sits in the same table, told
-- apart here; its matches are told apart through it.
ALTER TABLE etf2l_competition ADD COLUMN format TEXT NOT NULL DEFAULT 'highlander';
CREATE INDEX etf2l_competition_format ON etf2l_competition (format, season);

-- A league-sample log's format, so a 6v6 sample (P5) never feeds a
-- Highlander pool.
ALTER TABLE league_log ADD COLUMN format TEXT NOT NULL DEFAULT 'highlander';

-- A 6v6 player's role in one log (P3): 'pocket', 'roamer', 'flank_scout',
-- 'pocket_scout', 'demoman', 'medic', or 'off:<class>' for time on an
-- off-class. DERIVED from the log, except a role set by hand (`by_hand`),
-- which a re-run keeps.
--   method      how it was found: 'healspread', 'positions', 'demo', 'hand'
--   confidence  0 to 1, NULL when not measured
--   seconds     time played in the role
CREATE TABLE player_role (
    log_id     INTEGER NOT NULL,
    account_id INTEGER NOT NULL,
    role       TEXT    NOT NULL,
    method     TEXT    NOT NULL,
    confidence REAL,
    seconds    INTEGER,
    by_hand    INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (log_id, account_id, role)
) STRICT;

CREATE INDEX player_role_account ON player_role (account_id, role);
