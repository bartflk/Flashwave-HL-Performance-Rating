-- PLAN §12 step 2: Fight KAST. DERIVED by the fights pass (version 3), which
-- re-reads every log once to fill them.

ALTER TABLE fight_stat ADD COLUMN fights_present      INTEGER NOT NULL DEFAULT 0;  -- fights alive for
ALTER TABLE fight_stat ADD COLUMN fights_kast         INTEGER NOT NULL DEFAULT 0;  -- kill/assist, survived, or traded
ALTER TABLE fight_stat ADD COLUMN fights_kast_engaged INTEGER NOT NULL DEFAULT 0;  -- survival counts only after a shot
