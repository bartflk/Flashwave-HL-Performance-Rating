-- PLAN §12 step 4 (Q5): a pool per map, so a Vigil game is judged against
-- other Vigil games.
--
-- Measured over 13,568 performances in September 2026: a Sniper game averages
-- 0.90 on Vigil and 1.10 on Product. Three quarters of a standard deviation
-- of difference that belongs to the map, not to the player.
--
-- The primary key gains the map, so one class and component can hold a pool
-- per map alongside the general one. The empty string is the general pool,
-- not NULL: a primary key column here cannot be NULL, and SQLite would treat
-- two NULLs as different keys anyway. Every game is in that pool as well as
-- its map's -- the fallback has to be a real pool, not the leftovers of the
-- maps that missed the cut.
--
-- DERIVED by the full rating pass.

CREATE TABLE baseline_new (
    model_version TEXT NOT NULL,
    class         TEXT NOT NULL,
    component     TEXT NOT NULL,
    map           TEXT NOT NULL,      -- '': every map together
    n             INTEGER NOT NULL,
    sorted_values TEXT NOT NULL,      -- JSON array of numbers, ascending
    computed_at   TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (model_version, class, component, map)
) STRICT;

INSERT INTO baseline_new (model_version, class, component, map, n, sorted_values, computed_at)
SELECT model_version, class, component, '', n, sorted_values, computed_at FROM baseline;

DROP TABLE baseline;
ALTER TABLE baseline_new RENAME TO baseline;
