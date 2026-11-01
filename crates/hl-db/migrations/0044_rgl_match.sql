-- RGL's decided matches, kept (Flashy, November 2026: "store finished seasons
-- on disk"). RGL gives a division only its final rank, so a season's tables
-- are worked out from its matches, read one by one at RGL's pace: about 40 s
-- a season. A match with a winner, or forfeited, does not change, so it is
-- read once and kept here, as RGL's API answered it; a finished season opens
-- from here, a live one asks RGL only for the matches still to be played.
CREATE TABLE rgl_match (
    match_id  INTEGER PRIMARY KEY,
    body      TEXT NOT NULL,
    stored_at INTEGER NOT NULL
) STRICT;
