-- Q69 (Emiel): a mass import of one player's demos on one map from
-- demos.tf. A batch is one run; its demos are what was picked, with what
-- happened to each. `kept` is NULL until asked, then 1 (keep the files) or
-- 0 (deleted after reading): a kept batch's files are never swept away by
-- the "delete demos once read" setting.
CREATE TABLE mass_import (
    batch_id    INTEGER PRIMARY KEY,
    account_id  INTEGER NOT NULL,
    map         TEXT NOT NULL,
    class       TEXT,
    created_at  INTEGER NOT NULL,
    finished_at INTEGER,
    kept        INTEGER
) STRICT;

CREATE TABLE mass_import_demo (
    batch_id    INTEGER NOT NULL,
    demos_tf_id INTEGER NOT NULL,
    demo_id     INTEGER,
    log_id      INTEGER,
    -- queued | done | failed | skipped
    status      TEXT NOT NULL,
    error       TEXT,
    PRIMARY KEY (batch_id, demos_tf_id)
) STRICT;
