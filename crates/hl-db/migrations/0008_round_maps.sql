-- M8: the map of every round, including rounds of logs combined from
-- several maps under one made-up name.
--
-- part_raw is a SOURCE: logs.tf's JSON for the per-map logs a combined log was
-- built from, kept apart from log_raw so parts never become matches of their
-- own. round_map and log_segment are DERIVED and rebuilt from sources.

CREATE TABLE part_raw (
    log_id     INTEGER PRIMARY KEY,
    fetched_at INTEGER NOT NULL,
    json       TEXT NOT NULL
) STRICT;

CREATE TABLE round_map (
    log_id    INTEGER NOT NULL,
    round_num INTEGER NOT NULL,
    map       TEXT,                     -- NULL when nothing could tell
    source    TEXT,                     -- log | meta | part | geometry | window | neighbour
    PRIMARY KEY (log_id, round_num)
) STRICT;

-- Consecutive rounds on one map, in play order.
CREATE TABLE log_segment (
    log_id      INTEGER NOT NULL,
    seq         INTEGER NOT NULL,
    map         TEXT,
    first_round INTEGER NOT NULL,
    last_round  INTEGER NOT NULL,
    rounds      INTEGER NOT NULL,
    red_wins    INTEGER NOT NULL,       -- rounds won, stable teams
    blue_wins   INTEGER NOT NULL,
    PRIMARY KEY (log_id, seq)
) STRICT;
