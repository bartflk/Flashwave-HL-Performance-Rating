-- PLAN §14: the other half of what a demo says about a match — how the owner
-- died, and how much of their living time was spent scoped. DERIVED with
-- demo_aim by the aim pass, and dropped with it.

CREATE TABLE demo_death (
    log_id       INTEGER NOT NULL,
    demo_id      INTEGER NOT NULL,
    tick         INTEGER NOT NULL,
    at_raw       INTEGER,               -- the matching death in the log's clock
    killer       INTEGER,               -- account id, where the SteamID parsed
    killer_range REAL,                  -- NULL when the demo never carried them
    nearest_mate REAL,                  -- distance to the closest living teammate
    mates_near   INTEGER NOT NULL,      -- teammates within MATE_NEAR_UNITS
    scoped       INTEGER NOT NULL,      -- scoped in when it happened
    PRIMARY KEY (log_id, demo_id, tick)
) STRICT;

-- One row per match read: how the time was spent.
CREATE TABLE demo_life (
    log_id       INTEGER NOT NULL,
    demo_id      INTEGER NOT NULL,
    alive_ticks  INTEGER NOT NULL,
    scoped_ticks INTEGER NOT NULL,
    PRIMARY KEY (log_id, demo_id)
) STRICT;
