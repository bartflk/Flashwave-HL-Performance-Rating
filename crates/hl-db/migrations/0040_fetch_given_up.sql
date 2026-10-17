-- Q77 (Flashy): logs that will never download stop being retried and listed.
--
-- A log that drops.tf and logs.tf both answer "not found" for (HTTP 404 or
-- 410) has been deleted: retrying it can only fail. `gone` marks it so no
-- sync asks for it again, and `rescue_tried` that more.tf's copy has been
-- asked for once. Failures already stored are sorted by their saved error.

ALTER TABLE log_fetch_error ADD COLUMN gone INTEGER NOT NULL DEFAULT 0;
ALTER TABLE log_fetch_error ADD COLUMN rescue_tried INTEGER NOT NULL DEFAULT 0;

UPDATE log_fetch_error SET gone = 1 WHERE error LIKE '%HTTP 404%' OR error LIKE '%HTTP 410%';
