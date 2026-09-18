-- M4: demo files, their sidecar markers, links to logs, and each log's
-- placement on the real clock. All DERIVED: rebuilt by a demo scan from the
-- files on disk and the stored logs.

CREATE TABLE demo (
    demo_id       INTEGER PRIMARY KEY,
    path          TEXT NOT NULL UNIQUE,
    file_name     TEXT NOT NULL,
    playdemo_arg  TEXT NOT NULL,        -- relative to tf, no extension
    kind          TEXT NOT NULL,        -- pov | stv
    size_bytes    INTEGER NOT NULL,
    mtime         INTEGER NOT NULL,
    map           TEXT,
    server        TEXT,
    recorder      TEXT,
    playback_s    REAL NOT NULL,
    ticks         INTEGER NOT NULL,
    tick_rate     REAL,                 -- NULL when the header has no duration
    start_utc     REAL,                 -- recording start, unix seconds UTC
    filename_time TEXT,                 -- Demo Support timestamp, local wall time
    demos_tf_id   INTEGER,              -- set for STV demos fetched from demos.tf
    indexed_at    TEXT NOT NULL DEFAULT (datetime('now'))
) STRICT;

-- Tick-stamped markers from the Demo Support .json sidecar (killstreaks).
CREATE TABLE demo_event (
    demo_id INTEGER NOT NULL,
    seq     INTEGER NOT NULL,
    tick    INTEGER NOT NULL,
    name    TEXT NOT NULL,
    value   TEXT,
    PRIMARY KEY (demo_id, seq),
    FOREIGN KEY (demo_id) REFERENCES demo (demo_id) ON DELETE CASCADE
) STRICT;

CREATE TABLE demo_link (
    demo_id   INTEGER NOT NULL,
    log_id    INTEGER NOT NULL,
    method    TEXT NOT NULL,            -- exact | label | nomap | demos.tf
    log_share REAL NOT NULL,            -- share of the log's rounds inside the demo
    PRIMARY KEY (demo_id, log_id),
    FOREIGN KEY (demo_id) REFERENCES demo (demo_id) ON DELETE CASCADE
) STRICT;

CREATE INDEX demo_link_log ON demo_link (log_id);

-- Where a log's rounds sit on the real clock. logs.tf round times are the
-- server's local clock written as if UTC; offset_s converts them.
CREATE TABLE log_clock (
    log_id         INTEGER PRIMARY KEY,
    raw_start      INTEGER NOT NULL,
    raw_end        INTEGER NOT NULL,
    anchor_utc     INTEGER NOT NULL,
    anchor_kind    TEXT NOT NULL,       -- upload | parts
    offset_s       INTEGER NOT NULL,
    upload_delay_s INTEGER NOT NULL
) STRICT;
