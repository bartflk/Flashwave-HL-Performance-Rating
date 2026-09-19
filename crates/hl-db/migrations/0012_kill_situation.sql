-- PLAN §12 step 3: each counted kill's situation, from the killer's side.
-- DERIVED by the fights pass (version 4), with fight_stat; seq matches kill_event.

CREATE TABLE kill_situation (
    log_id INTEGER NOT NULL,
    seq    INTEGER NOT NULL,
    diff   INTEGER NOT NULL,  -- killer's team alive minus the victim's, just before, -4..4
    adv    INTEGER NOT NULL,  -- uber advantage just before: 1 killer's team, -1 victim's, 0 neither
    PRIMARY KEY (log_id, seq)
) STRICT;
