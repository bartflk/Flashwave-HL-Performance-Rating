-- PLAN §14: the crosshair's path over the second before each kill, so a flick
-- and its overshoot can be drawn rather than only its endpoint. A JSON array
-- of [sideways, vertical] degrees, oldest first, ending at the shot.
-- DERIVED with the rest of demo_aim; the aim pass moves to version 5.

ALTER TABLE demo_aim ADD COLUMN path TEXT;
