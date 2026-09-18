-- M1: the log index, raw log blobs, and normalized match data.
--
-- Two kinds of table live here and the distinction matters:
--   SOURCE  log_index.*_json, log_raw   never rewritten except by a refetch
--   DERIVED everything else             rebuildable from the sources at any time

-- Every log we know exists for the owner, from either trends.tf or the logs.tf
-- search list. A row exists here before its detail has been fetched.
CREATE TABLE log_index (
    log_id          INTEGER PRIMARY KEY,
    title           TEXT,
    map             TEXT,
    played_at       INTEGER,            -- unix seconds
    duration_s      INTEGER,
    player_count    INTEGER,

    -- Source rows, verbatim. Everything below them is derived.
    trends_json     TEXT,
    logstf_json     TEXT,

    format          TEXT,               -- highlander | sixes | prolander | other | NULL (unknown yet)
    league          TEXT,               -- etf2l | rgl | ... | NULL
    etf2l_match_id  INTEGER,
    demos_tf_id     INTEGER,
    -- A combined log lists the per-round logs it was built from. The combined
    -- log is the one to keep; its parts are superseded by it.
    duplicate_of    TEXT,               -- JSON array of log ids, as trends.tf reports it
    superseded_by   INTEGER,            -- non-NULL: excluded from every aggregate
    classified_by   TEXT,               -- trends | heuristic | manual
    format_override TEXT,               -- set by hand; wins over everything

    updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
) STRICT;

CREATE INDEX log_index_played_at ON log_index (played_at DESC);
CREATE INDEX log_index_superseded ON log_index (superseded_by);

-- Full logs.tf JSON. The single most valuable table: everything in the match_*
-- tables is rebuilt from this by `reprocess` without touching the network.
CREATE TABLE log_raw (
    log_id     INTEGER PRIMARY KEY,
    fetched_at TEXT NOT NULL DEFAULT (datetime('now')),
    json       TEXT NOT NULL
) STRICT;

-- Fetch failures, so a sync can resume and a permanently broken log does not
-- get retried forever.
CREATE TABLE log_fetch_error (
    log_id          INTEGER PRIMARY KEY,
    attempts        INTEGER NOT NULL DEFAULT 1,
    last_attempt_at TEXT NOT NULL DEFAULT (datetime('now')),
    error           TEXT NOT NULL
) STRICT;

-- ---------------------------------------------------------------------------
-- Normalized match data (DERIVED from log_raw)
-- ---------------------------------------------------------------------------

CREATE TABLE match (
    log_id        INTEGER PRIMARY KEY,
    title         TEXT,
    map           TEXT,
    played_at     INTEGER,
    duration_s    INTEGER,
    red_score     INTEGER,
    blue_score    INTEGER,
    round_count   INTEGER,

    -- logs.tf capability flags. A stat whose flag is 0 was not recorded for
    -- this log: it is missing, not zero. Old logs lack airshots, some lack
    -- accuracy; treating those as zeros would corrupt every average.
    has_real_damage INTEGER NOT NULL,
    has_accuracy    INTEGER NOT NULL,
    has_hs          INTEGER NOT NULL,
    has_hs_hit      INTEGER NOT NULL,
    has_bs          INTEGER NOT NULL,
    has_cp          INTEGER NOT NULL,
    has_dt          INTEGER NOT NULL,
    has_as          INTEGER NOT NULL,
    has_hr          INTEGER NOT NULL
) STRICT;

CREATE TABLE match_player (
    log_id        INTEGER NOT NULL,
    account_id    INTEGER NOT NULL,
    name          TEXT,
    team          TEXT NOT NULL,        -- Red | Blue
    main_class    TEXT,                 -- most playtime; NULL if none recorded
    time_s        INTEGER NOT NULL,     -- total playtime across classes

    kills         INTEGER NOT NULL,
    deaths        INTEGER NOT NULL,
    assists       INTEGER NOT NULL,
    suicides      INTEGER NOT NULL,
    dmg           INTEGER NOT NULL,
    dmg_real      INTEGER NOT NULL,
    dt            INTEGER NOT NULL,
    dt_real       INTEGER NOT NULL,
    hr            INTEGER NOT NULL,     -- health received
    heal          INTEGER NOT NULL,
    ubers         INTEGER NOT NULL,
    drops         INTEGER NOT NULL,
    headshots     INTEGER NOT NULL,     -- headshot kills
    headshots_hit INTEGER NOT NULL,     -- headshots landed
    backstabs     INTEGER NOT NULL,
    medkits       INTEGER NOT NULL,
    medkits_hp    INTEGER NOT NULL,
    sentries      INTEGER NOT NULL,
    cpc           INTEGER NOT NULL,     -- points captured
    ic            INTEGER NOT NULL,     -- intel captured
    lks           INTEGER NOT NULL,     -- longest killstreak
    airshots      INTEGER NOT NULL,

    PRIMARY KEY (log_id, account_id),
    FOREIGN KEY (log_id) REFERENCES match (log_id) ON DELETE CASCADE
) STRICT;

CREATE INDEX match_player_account ON match_player (account_id);

CREATE TABLE match_player_class (
    log_id     INTEGER NOT NULL,
    account_id INTEGER NOT NULL,
    class      TEXT NOT NULL,
    time_s     INTEGER NOT NULL,
    kills      INTEGER NOT NULL,
    assists    INTEGER NOT NULL,
    deaths     INTEGER NOT NULL,
    dmg        INTEGER NOT NULL,
    PRIMARY KEY (log_id, account_id, class),
    FOREIGN KEY (log_id) REFERENCES match (log_id) ON DELETE CASCADE
) STRICT;

-- Kills, deaths and assists broken down by the *other* player's class. This is
-- what makes impact weighting possible: killing a Medic is not killing a Pyro.
--   kills   = kills of players on `other_class`
--   deaths  = deaths to players on `other_class`
--   assists = assists on kills of players on `other_class`
CREATE TABLE match_class_vs (
    log_id      INTEGER NOT NULL,
    account_id  INTEGER NOT NULL,
    other_class TEXT NOT NULL,
    kills       INTEGER NOT NULL DEFAULT 0,
    deaths      INTEGER NOT NULL DEFAULT 0,
    assists     INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (log_id, account_id, other_class),
    FOREIGN KEY (log_id) REFERENCES match (log_id) ON DELETE CASCADE
) STRICT;

CREATE TABLE match_round (
    log_id     INTEGER NOT NULL,
    round_num  INTEGER NOT NULL,        -- 1-based
    start_time INTEGER,                 -- unix seconds
    length_s   INTEGER,
    winner     TEXT,                    -- Red | Blue | NULL
    firstcap   TEXT,
    red_kills  INTEGER, blue_kills INTEGER,
    red_dmg    INTEGER, blue_dmg   INTEGER,
    red_ubers  INTEGER, blue_ubers INTEGER,
    PRIMARY KEY (log_id, round_num),
    FOREIGN KEY (log_id) REFERENCES match (log_id) ON DELETE CASCADE
) STRICT;

-- Timestamped round events. Only these kinds exist in logs.tf: pointcap,
-- charge, drop, medic_death, round_win. Notably there are no general kill
-- events -- the only kills with a timestamp and a killer are medic deaths.
CREATE TABLE match_event (
    log_id    INTEGER NOT NULL,
    round_num INTEGER NOT NULL,
    seq       INTEGER NOT NULL,         -- order within the round
    at_s      INTEGER NOT NULL,         -- seconds into the round
    kind      TEXT NOT NULL,
    team      TEXT,
    player    INTEGER,                  -- account id: the medic, for charge/drop/medic_death
    killer    INTEGER,                  -- account id: medic_death only
    medigun   TEXT,
    point     INTEGER,
    PRIMARY KEY (log_id, round_num, seq),
    FOREIGN KEY (log_id) REFERENCES match (log_id) ON DELETE CASCADE
) STRICT;
