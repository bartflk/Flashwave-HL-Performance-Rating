-- RGL's other answers worth keeping, by API path (Flashy, November 2026: "make
-- sure to properly use the api to not overburden them"). A finished season's
-- page, its divisions and their teams, and a team whose season is over do not
-- change, so they are read once and kept. A match still to be played is kept
-- too, with when it was read: RGL's list of matches changed since then says
-- which to ask for again, instead of asking for every one.
CREATE TABLE rgl_answer (
    path      TEXT PRIMARY KEY,
    body      TEXT NOT NULL,
    stored_at INTEGER NOT NULL
) STRICT;
