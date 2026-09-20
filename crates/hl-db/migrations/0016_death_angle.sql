-- PLAN §14: where the player who killed you stood relative to where you were
-- looking, in degrees: positive is to your right and above your crosshair, and
-- 180 sideways is directly behind you. NULL when the demo never carried them.
-- DERIVED with the rest of demo_death; the aim pass moves to version 4.

ALTER TABLE demo_death ADD COLUMN killer_dx_deg REAL;
ALTER TABLE demo_death ADD COLUMN killer_dy_deg REAL;
