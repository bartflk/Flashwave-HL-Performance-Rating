-- Bookkeeping for the sync audit (November 2026). SOURCE bookkeeping only:
-- nothing here is derived, and nothing here is needed to rebuild.

-- A log the sources did not have, with when we last asked and how many times:
-- the sync asks again after a day, then a week, then a month, instead of on
-- every sync for ever (the log stays in the queue otherwise). Removed when
-- the log is stored, or when a person asks for a retry by hand.
CREATE TABLE log_source_miss (
    log_id       INTEGER PRIMARY KEY,
    misses       INTEGER NOT NULL DEFAULT 1,
    last_miss_at INTEGER NOT NULL DEFAULT (unixepoch())
);

-- A demo whose parse failed, with the size and modified time of the file that
-- failed: the same file is not read again (about 16 s of CPU each time) until
-- it changes.
CREATE TABLE demo_parse_failed (
    demo_id    INTEGER PRIMARY KEY,
    size_bytes INTEGER NOT NULL,
    mtime      INTEGER NOT NULL,
    failed_at  INTEGER NOT NULL DEFAULT (unixepoch()),
    error      TEXT    NOT NULL
);

-- A raw log whose rounds did not line up with logs.tf's: its kills were stored
-- on the server's own clock, an unknown number of hours off, so nothing may
-- join them to a demo's clock.
CREATE TABLE rawlog_unaligned (
    log_id   INTEGER PRIMARY KEY,
    noted_at INTEGER NOT NULL DEFAULT (unixepoch())
);
