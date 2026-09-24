-- PLAN: which logs are officials, known before they are downloaded.
--
-- Whether a log is an official is otherwise only settled after fetching it,
-- by matching ETF2L rosters against its player list — and trends.tf tags only
-- 19 of this account's 32 older ones. A retention window that keeps "recent,
-- or an official" therefore needs an earlier signal.
--
-- ETF2L gives each match a scheduled time, and across 56 linked officials
-- every log started between 10 and 166 minutes after it, never before. So a
-- kept log inside [scheduled, scheduled + 3h] is that match: 56 of 56 found,
-- 7 other logs swept in. The ETF2L match's own map list is no use here — a
-- combined log's map is free text the uploader typed ("prod,vigil,ash").
--
-- DERIVED on every sync, after the ETF2L pass and before the fetch queue.

ALTER TABLE log_index ADD COLUMN etf2l_time_match INTEGER;
