-- Sentries and other buildings each player destroyed, by the class they were
-- on at the time, from the server log's "killedobject" lines (Flashy, October
-- 2026: "is there a way to see if a player kills a sentry like a sniper?").
-- NULL where the log has no server log, or until its server log is read
-- again for them.
ALTER TABLE log_stat ADD COLUMN sentry_kills INTEGER;
ALTER TABLE log_stat ADD COLUMN building_kills INTEGER;
