-- M3: rating baselines and per-performance ratings. Both DERIVED: rebuilt
-- from the stored logs by `rate`, which runs at the end of every sync and
-- every reprocess.
--
-- Versioned by model, so a new model can be introduced without its numbers
-- silently mixing with the old one's.

-- The pool each rating is measured against: every other player's value for
-- one component on one class, sorted. Percentiles are looked up in it exactly.
CREATE TABLE baseline (
    model_version TEXT NOT NULL,
    class         TEXT NOT NULL,
    component     TEXT NOT NULL,
    n             INTEGER NOT NULL,
    sorted_values TEXT NOT NULL,        -- JSON array of numbers, ascending
    computed_at   TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (model_version, class, component)
) STRICT;

-- One rated performance: a player's time on their main class in one match.
CREATE TABLE rating (
    log_id        INTEGER NOT NULL,
    account_id    INTEGER NOT NULL,
    model_version TEXT NOT NULL,
    class         TEXT NOT NULL,
    score         REAL NOT NULL,        -- 0-100
    minutes       REAL NOT NULL,
    parts         TEXT NOT NULL,        -- JSON: the full breakdown
    PRIMARY KEY (log_id, account_id, model_version)
) STRICT;

CREATE INDEX rating_player ON rating (account_id, model_version, class);
