-- Every player's numbers in every log, for the season rankings page, the
-- profile panels and the season awards (the MVP research, October 2026).
--
-- One row per player and class played in a log. Kills, deaths, assists and
-- damage are logs.tf's per-class figures; what logs.tf records per player
-- rather than per class (damage taken, heals, ubers, airshots, headshots,
-- backstabs, caps, medkits, the Medic's numbers) and what the server log
-- gives (fights, impact) sit on the row of the class the player spent most
-- of the log on (`main` = 1), and are 0 on the others. Columns from the
-- server log are NULL where the log has none.
--
-- Covers the league sample's officials and the owner's own matches alike:
-- both are logs.tf logs, keyed by log id.
--
-- DERIVED: written when a log is rated, from its logs.tf JSON and raw
-- server log. The built-in league snapshot carries it, since it carries
-- neither of those.

CREATE TABLE log_stat (
    log_id          INTEGER NOT NULL,
    account_id      INTEGER NOT NULL,
    class           TEXT    NOT NULL,
    main            INTEGER NOT NULL,
    time_s          INTEGER NOT NULL,
    kills           INTEGER NOT NULL,
    deaths          INTEGER NOT NULL,
    assists         INTEGER NOT NULL,
    dmg             INTEGER NOT NULL,
    dt              INTEGER NOT NULL,
    heal            INTEGER NOT NULL,
    ubers           INTEGER NOT NULL,
    drops           INTEGER NOT NULL,
    airshots        INTEGER NOT NULL,
    headshots       INTEGER NOT NULL,
    backstabs       INTEGER NOT NULL,
    caps            INTEGER NOT NULL,
    medkits         INTEGER NOT NULL,
    medkits_hp      INTEGER NOT NULL,
    longest_streak  INTEGER NOT NULL,
    -- The Medic's: advantages lost, and average seconds to build and to use a charge.
    adv_lost        INTEGER NOT NULL,
    build_s         REAL,
    use_s           REAL,
    -- From the server log's fights pass.
    opening_kills   INTEGER,
    opening_deaths  INTEGER,
    first_picks     INTEGER,
    trade_kills     INTEGER,
    traded_deaths   INTEGER,
    cleanup_kills   INTEGER,
    forces          INTEGER,
    fights_present  INTEGER,
    fights_kast     INTEGER,
    -- Impact: round-win chance added, in rounds (round_impact.rs).
    impact          REAL,
    impact_attack   REAL,
    impact_defence  REAL,
    kills_attack    INTEGER,
    deaths_attack   INTEGER,
    kills_defence   INTEGER,
    deaths_defence  INTEGER,
    secs_attack     INTEGER,
    secs_defence    INTEGER,
    down_kills      INTEGER,
    multi2          INTEGER,
    multi3          INTEGER,
    PRIMARY KEY (log_id, account_id, class)
) STRICT;

CREATE INDEX log_stat_account ON log_stat (account_id);

-- Kills and damage per weapon, from logs.tf's per-class breakdown.
CREATE TABLE log_weapon (
    log_id     INTEGER NOT NULL,
    account_id INTEGER NOT NULL,
    class      TEXT    NOT NULL,
    weapon     TEXT    NOT NULL,
    kills      INTEGER NOT NULL,
    dmg        INTEGER NOT NULL,
    shots      INTEGER NOT NULL,
    hits       INTEGER NOT NULL,
    PRIMARY KEY (log_id, account_id, class, weapon)
) STRICT;

CREATE INDEX log_weapon_account ON log_weapon (account_id);
