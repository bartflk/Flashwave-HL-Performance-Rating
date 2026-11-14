-- The 6v6 league sample (the 6v6 plan, P5): every ETF2L 6v6 official's
-- logs, as trends.tf lists them, for the 6v6 rating's pools. Its own table:
-- Highlander's sample (league_log, 0033) is read by Highlander's download
-- queue and league rating, and a 6v6 log there would be rated as Highlander.
-- SOURCE data: trends.tf's listing, and each log's JSON from drops.tf.
--   json_attempts  tries that found nothing, so a lost log stops being asked
CREATE TABLE sixes_league_log (
    log_id         INTEGER PRIMARY KEY,
    etf2l_match_id INTEGER NOT NULL,
    map            TEXT    NOT NULL,
    played_at      INTEGER NOT NULL,
    duration_s     INTEGER,
    title          TEXT,
    json           TEXT,
    json_source    TEXT,
    json_attempts  INTEGER NOT NULL DEFAULT 0
) STRICT;

CREATE INDEX sixes_league_log_todo ON sixes_league_log (json_source, played_at);
