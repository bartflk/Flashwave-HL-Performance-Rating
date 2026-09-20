-- PLAN §14: what a demo says about the aim behind each of the owner's kills.
-- DERIVED from the stored demos, rebuilt by the aim pass; aim_log records
-- which logs have been read, and at which version of the pass.

CREATE TABLE demo_aim (
    log_id      INTEGER NOT NULL,
    demo_id     INTEGER NOT NULL,
    tick        INTEGER NOT NULL,       -- the demo's own kill tick
    at_raw      INTEGER,                -- the matching kill in the log's clock
    victim      INTEGER,                -- account id, where the SteamID parsed
    error_deg   REAL NOT NULL,          -- view to the victim's head, at the shot
    before_deg  REAL NOT NULL,          -- the same, one second earlier
    flick_deg   REAL NOT NULL,          -- how far the view turned in the last half second
    range_units REAL NOT NULL,
    height      REAL NOT NULL,          -- how far above the shooter the victim stood
    victim_seen INTEGER NOT NULL,       -- the demo carried both players throughout
    headshot    INTEGER NOT NULL,
    PRIMARY KEY (log_id, demo_id, tick)
) STRICT;

CREATE TABLE aim_log (
    log_id  INTEGER PRIMARY KEY,
    version INTEGER NOT NULL,
    kills   INTEGER NOT NULL            -- rows stored, for a quick "nothing found" answer
) STRICT;
