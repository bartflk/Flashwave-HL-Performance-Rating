-- PLAN §12 step 1: deaths in context. DERIVED by the fights pass (version 2),
-- which re-reads every log once to fill them.

ALTER TABLE fight_stat ADD COLUMN traded_deaths     INTEGER NOT NULL DEFAULT 0;  -- own team killed back within 3 s
ALTER TABLE fight_stat ADD COLUMN deaths_to_sniper  INTEGER NOT NULL DEFAULT 0;
ALTER TABLE fight_stat ADD COLUMN deaths_to_flank   INTEGER NOT NULL DEFAULT 0;  -- Scout, Spy, Soldier
ALTER TABLE fight_stat ADD COLUMN deaths_to_combo   INTEGER NOT NULL DEFAULT 0;  -- Medic, Demoman, Heavy, Pyro
ALTER TABLE fight_stat ADD COLUMN stationary_deaths INTEGER NOT NULL DEFAULT 0;  -- near a spot killed from twice this life
