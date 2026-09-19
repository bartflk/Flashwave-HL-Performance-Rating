-- M6: the raw server log behind each logs.tf page, and every kill in it.
--
-- rawlog is a SOURCE: the zip exactly as logs.tf served it, never altered.
-- kill_event and chat_event are DERIVED from it and rebuilt on reprocess.

CREATE TABLE rawlog (
    log_id     INTEGER PRIMARY KEY,
    fetched_at INTEGER NOT NULL,
    size_bytes INTEGER NOT NULL,         -- compressed
    zip        BLOB NOT NULL
) STRICT;

-- Logs logs.tf has no raw file for, so a sync does not ask again every time.
CREATE TABLE rawlog_missing (
    log_id     INTEGER PRIMARY KEY,
    checked_at INTEGER NOT NULL,
    reason     TEXT NOT NULL
) STRICT;

-- Every kill line. Times are the server's clock written as if UTC, the same
-- frame as logs.tf round start times; log_clock.offset_s converts them.
CREATE TABLE kill_event (
    log_id       INTEGER NOT NULL,
    seq          INTEGER NOT NULL,
    at_raw       INTEGER NOT NULL,
    round_num    INTEGER NOT NULL,       -- 0 before the first round starts
    live         INTEGER NOT NULL,       -- inside a round: what logs.tf counts
    killer       INTEGER NOT NULL,
    killer_team  TEXT,                   -- Red | Blue, the colour at that moment
    killer_class TEXT,
    victim       INTEGER NOT NULL,
    victim_team  TEXT,
    victim_class TEXT,
    weapon       TEXT NOT NULL,
    custom       TEXT,                   -- headshot | backstab | feign_death | ...
    assister     INTEGER,
    kx INTEGER, ky INTEGER, kz INTEGER,  -- killer position
    vx INTEGER, vy INTEGER, vz INTEGER,  -- victim position
    PRIMARY KEY (log_id, seq)
) STRICT;

CREATE INDEX kill_event_killer ON kill_event (killer);
CREATE INDEX kill_event_victim ON kill_event (victim);

CREATE TABLE chat_event (
    log_id    INTEGER NOT NULL,
    seq       INTEGER NOT NULL,
    at_raw    INTEGER NOT NULL,
    account   INTEGER,                   -- NULL for the server console
    team_chat INTEGER NOT NULL,
    message   TEXT NOT NULL,
    PRIMARY KEY (log_id, seq)
) STRICT;
