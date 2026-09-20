-- PLAN §14: which way the crosshair was off, not just how far — so a kill can
-- be drawn on a target. Positive dx is right of the head, positive dy above
-- it. DERIVED with the rest of demo_aim; the aim pass moves to version 3.

ALTER TABLE demo_aim ADD COLUMN dx_deg        REAL NOT NULL DEFAULT 0;  -- at the shot
ALTER TABLE demo_aim ADD COLUMN dy_deg        REAL NOT NULL DEFAULT 0;
ALTER TABLE demo_aim ADD COLUMN before_dx_deg REAL NOT NULL DEFAULT 0;  -- a second before
ALTER TABLE demo_aim ADD COLUMN before_dy_deg REAL NOT NULL DEFAULT 0;
