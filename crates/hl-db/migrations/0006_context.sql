-- M5: ETF2L context and what kind of game each match was.
--
-- etf2l_raw is a SOURCE (kept, refetched only while a match is fresh); the
-- rest is DERIVED and rebuilt by the context pass with no network.

CREATE TABLE etf2l_raw (
    kind       TEXT NOT NULL,               -- player | results | match
    id         INTEGER NOT NULL,            -- player id, results page, match id
    fetched_at INTEGER NOT NULL,            -- unix seconds
    json       TEXT NOT NULL,
    PRIMARY KEY (kind, id)
) STRICT;

CREATE TABLE etf2l_match (
    match_id       INTEGER PRIMARY KEY,
    competition_id INTEGER,
    competition    TEXT,                    -- "Highlander Season 36 (Autumn 2026): High"
    comp_type      TEXT,                    -- Highlander | 6v6 | ...
    category       TEXT,                    -- "Highlander Season", "Highlander Cup", ...
    division       TEXT,                    -- High, Open, Division 2, ...
    tier           INTEGER,                 -- 1 is the top tier
    week           INTEGER,
    round          TEXT,                    -- "Week 3", "Round 1", "Semi Final"
    time           INTEGER,                 -- scheduled, unix seconds
    clan1_id       INTEGER,
    clan1_name     TEXT,
    clan2_id       INTEGER,
    clan2_name     TEXT,
    r1             INTEGER,                 -- ETF2L's score, not rounds won
    r2             INTEGER,
    default_win    INTEGER NOT NULL DEFAULT 0,
    maps           TEXT                     -- JSON array
) STRICT;

-- Who was registered to each side of an official. Mercs appear with team_id
-- NULL. Used to tell which side the owner played for, to find officials
-- trends.tf did not tag, and to name the teams in scrims.
CREATE TABLE etf2l_roster (
    match_id   INTEGER NOT NULL,
    account_id INTEGER NOT NULL,
    team_id    INTEGER,
    name       TEXT,
    PRIMARY KEY (match_id, account_id)
) STRICT;

-- One row per kept Highlander match the owner played in.
CREATE TABLE match_context (
    log_id         INTEGER PRIMARY KEY,
    kind           TEXT NOT NULL,           -- official | scrim | pug
    etf2l_match_id INTEGER,
    link_method    TEXT,                    -- trends | roster (officials only)
    team_id        INTEGER,                 -- the owner's ETF2L team, when known
    team_name      TEXT,
    opp_team_id    INTEGER,
    opp_team_name  TEXT,
    regulars       INTEGER NOT NULL         -- teammates who play with the owner regularly
) STRICT;

CREATE INDEX match_context_kind ON match_context (kind);
CREATE INDEX match_context_team ON match_context (team_id);
