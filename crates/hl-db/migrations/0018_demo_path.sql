-- PLAN §14: where the owner walked, one route per life, from their own demo.
-- DERIVED with the rest of the aim pass and dropped with it.
--
-- The points are a JSON array of [tick, x, y, z] in map units, about four a
-- second. One match is roughly 40 KB, so a full history is a couple of
-- megabytes; keeping them as one blob per life beats a row per point.

CREATE TABLE demo_path (
    log_id     INTEGER NOT NULL,
    demo_id    INTEGER NOT NULL,
    seq        INTEGER NOT NULL,       -- the life's order within the demo
    from_tick  INTEGER NOT NULL,
    to_tick    INTEGER NOT NULL,
    round_num  INTEGER,                -- the round the life started in
    died       INTEGER NOT NULL,       -- ended in a death, not a round end
    points     TEXT NOT NULL,
    PRIMARY KEY (log_id, demo_id, seq)
) STRICT;
