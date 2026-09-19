-- PLAN §11 B and D: every player's kills in context, per match. DERIVED from
-- the stored raw logs by the fights pass; rebuilt when its version changes.

-- Logs the pass has read, and with which version of it.
CREATE TABLE fight_log (
    log_id  INTEGER PRIMARY KEY,
    version INTEGER NOT NULL
) STRICT;

CREATE TABLE fight_stat (
    log_id             INTEGER NOT NULL,
    account_id         INTEGER NOT NULL,
    rounds             INTEGER NOT NULL,   -- rounds alive in at some point
    kills              INTEGER NOT NULL,
    deaths             INTEGER NOT NULL,
    opening_kills      INTEGER NOT NULL,   -- first kill of a fight
    opening_deaths     INTEGER NOT NULL,
    first_picks        INTEGER NOT NULL,   -- first kill of a round
    first_deaths       INTEGER NOT NULL,
    traded_kills       INTEGER NOT NULL,   -- own team lost someone within 3 s
    died_after_kill    INTEGER NOT NULL,   -- the killer died within 3 s
    trade_kills        INTEGER NOT NULL,   -- avenged a teammate within 3 s
    cleanup_kills      INTEGER NOT NULL,   -- own team already up a player
    charged_picks      INTEGER NOT NULL,   -- combo victim, their charge ready
    drops              INTEGER NOT NULL,   -- Medic victim holding a ready charge
    forces             INTEGER NOT NULL,   -- enemy pops under this player's damage
    deaths_before_uber INTEGER NOT NULL,   -- 10 s before own team's pop
    deaths_during_uber INTEGER NOT NULL,
    deaths_after_uber  INTEGER NOT NULL,   -- 10 s after it ended
    PRIMARY KEY (log_id, account_id)
) STRICT;

CREATE INDEX fight_stat_player ON fight_stat (account_id);
