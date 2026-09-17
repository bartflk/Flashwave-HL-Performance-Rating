-- M0: configuration and player identity only.
--
-- Match, demo and rating tables arrive with the milestones that read them --
-- writing a table before there is code to fill it just bakes in guesses.

CREATE TABLE app_config (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
) STRICT;

-- Canonical identity for everyone we ever see, keyed by the 32-bit account id.
-- steamid64 exceeds SQLite's comfortable integer display range and JavaScript's
-- safe integer range, so it is stored as TEXT and derived, never parsed back.
CREATE TABLE player (
    account_id   INTEGER PRIMARY KEY,
    steamid64    TEXT NOT NULL UNIQUE,
    steamid3     TEXT NOT NULL UNIQUE,
    display_name TEXT,
    is_me        INTEGER NOT NULL DEFAULT 0,
    updated_at   TEXT NOT NULL DEFAULT (datetime('now'))
) STRICT;

-- There is exactly one "me". A partial unique index enforces it in the schema
-- rather than in application code.
CREATE UNIQUE INDEX player_single_me ON player (is_me) WHERE is_me = 1;

-- Cursors and error state for each ingest source (logs.tf, demos, RGL).
CREATE TABLE sync_state (
    source       TEXT PRIMARY KEY,
    cursor       TEXT,
    last_run_at  TEXT,
    last_error   TEXT
) STRICT;
