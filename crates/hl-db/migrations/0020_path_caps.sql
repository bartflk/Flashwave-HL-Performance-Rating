-- PLAN §14: the points captured during each life, so a long life reads as
-- what it won rather than only how long it lasted. A JSON array of
-- [seconds into the life, point number], the player's own team only.
-- DERIVED with the rest of the aim pass (version 10).

ALTER TABLE demo_path ADD COLUMN caps TEXT;
