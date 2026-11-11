# How the Flashwave.tf rating works

**Model v14. Written to be argued with** — if something here is wrong about
how Highlander is played, it is wrong in the app too, and the numbers in it
are the place to say so.

The tables below (victim values, attack/defend maps, every class's model)
were generated from the app's weights file
(`crates/hl-rating/src/weights.default.toml`), not written from memory. The
same file carries the reasoning for each change inline, and can be edited
without rebuilding the app.

---

## 1. The short version

A rating is **one player, on one class, in one match**.

```
for each component:   percentile against everyone else who played that class
score (0–100):        weighted average of those percentiles
rating (shown):       1.00 + (score − class mean) / class sd × 0.25
```

- **1.00** is an average game *of that class*.
- **One standard deviation is 0.25**, so 1.25 is a very good game and 0.75 a
  poor one.
- Floored at 0, nothing caps the top. Best real single games land near 1.70.

This is HLTV's idea: express a performance as how many standard deviations
it is from average, and centre that on 1.00.

## 2. What it is measured against

**The pool is the league.** Every ETF2L official in the league sample, plus
the owner's own officials, with the games' numbers read from logs.tf and, where
there is one, the raw server log.

- **Officials only (v14).** Once there are 1,000 official performances to
  measure from, the owner's scrims and pugs are left out of the pool: it is
  meant to describe a league game, and they would shift every percentile.
  They are still rated against it. Below that (a fresh install with no league
  sample yet) every game is in the pool, as before.
- **The owner is in the pool.** One player among thousands, not half of a
  class's pool as in their own matches alone; they are no longer left out.
- **Per map where there is enough data.** A pool needs **100+ performances**
  on that map for that class, otherwise it falls back to all maps. Vigil
  averaged 0.90 and Product 1.10 before this, three quarters of a deviation
  that belonged to the map and not the player.
- **A scale of its own per class (v14).** A Sniper's weighted percentiles
  spread wider than a Medic's, whose model has more parts that move together.
  On one shared mean and spread a Sniper's good game looked better than a
  Medic's equally good one. Each class has its own mean and spread, and a
  game with no server log (rated on half a model) has its own too; one with
  too few games to measure takes the next wider scale.
- **Minimum 5 minutes** on the class, or it is not rated at all — a spawn
  swap is not a performance.
- **Main class only.** Deaths and damage are the main class's, over its
  minutes. logs.tf records kills, assists, heals, ubers and caps per player,
  not per class, and so do the server-log numbers (round impact, fights): those
  are read over the whole game, so twelve minutes of Spy and ten of Sniper are
  not rated as twenty-two minutes of kills in twelve (v14).

**Known limitation:** the pool is the league, not the player's own level. A
Premiership player and a low-Open player are not rated against different
standards, and the app says so rather than pretending otherwise.

## 3. What a "kill" is worth

Not all kills count the same. A kill is valued by **who died**:

| victim | worth | why |
|---|---:|---|
| Medic | **3** | a Medic pick decides the fight |
| Demoman | 2.2 | the team's damage |
| Sniper | 1.8 | denying their picks |
| Pyro | 1.5 | with their Pyro dead you can spam freely |
| Heavy | 1.4 |  |
| Spy | 1.3 | important, rarely decides a teamfight |
| Soldier | 1.2 |  |
| Scout | 1.15 |  |
| Engineer | 1.1 |  |

On attack/defence maps (every payload map, plus Steel, Gravelpit, Dustbowl, Egypt, Gorge, Junction, Mountainlab) killing a
**defending Engineer is 1.6** and a **defending Scout 1** — the
sentry holds the defence, and the Scout matters less defending than attacking.
One list of those maps (`hl_core::maps`) serves the log's attack/defend flag,
the situation tables and these values.

An **assist counts for 0.5** of the same kill.

**These values are an opinion, and the honest caveat is that the app cannot
test them.** Five different values for the Scout on KOTH, and a Medic KOTH
premium, all moved match-winner prediction by less than half a point. They
are held to the eye test and to how Highlander is actually played — which is
exactly why they need reviewing by players.

### Kills in context

A kill while already **3 up counts 0.75**, and **4+ up counts 0.5**.
Measured over 193,626 kills: a kill at four up lifts the chance of winning
the round by a fifth as much as one at even numbers. Clean-ups are worth
less. No other state adjustment survived testing.

### Round impact (v12)

Half of every model except the Medic's (§5). Every counted kill moves the
killer's side from *own v enemy* alive to *own v enemy−1*; how much that is
worth is read from a table of how often a side at those numbers, with or
without the uber advantage, went on to win the round (measured on the league
sample, ~1.2 million kills). The killer is credited with the change (three
quarters of it when someone assisted, the assister the rest) and the player
who died loses it. A kill of a Medic who held the advantage is valued into the
state it leaves, with the advantage gone (v14).

## 4. The components

| component | what it measures | unit |
|---|---|---|
| `round_impact` | round-win chance the kills added and the deaths gave away | rounds per 10 min |
| `impact_kills` | kills, each valued by victim class/map/side | per 10 min |
| `situation_kills` | the same, also scaled by the clean-up discount | per 10 min |
| `impact_assists` | assists, valued the same way, at 0.5 | per 10 min |
| `medic_picks` | enemy Medics killed | per 10 min |
| `deaths`, `deaths_attack_defend` | deaths (**lower is better**), on KOTH/5CP and on payload/attack-defend maps | per 10 min |
| `untraded_deaths` | deaths your team did **not** trade within 3 s (lower better) | per 10 min |
| `flank_deaths` | deaths to a Scout, Spy or Soldier (lower better) | per 10 min |
| `untraded` | share of *your* kills the enemy did not trade back | % of kills |
| `opening` | first kills of a fight, minus first deaths | net per 10 min |
| `fight_kast_engaged` | share of fights with a kill, assist, survival or traded death | % of fights |
| `fight_swing_shared` | the win chance a kill added, shared with everyone who damaged the victim in the 5 s before | win % per 10 min |
| `caps_contested` | each capture weighted by the enemies alive to stop it (0-9) | defenders per 10 min |
| `heal`, `ubers`, `drops` | Medic's own numbers (drops lower better) | per min / per 10 min |
| `backstabs` | Spy | per 10 min |
| `duel` | kills on the enemy Sniper minus deaths to them | per 10 min |
| `headshot_share` | share of kills that were headshots | % |
| `dpm` | damage per minute | per min |

A fight = 10 s with nobody dying. A trade = a kill back within 3 s. Both
windows were measured on ~195,000 kills, not guessed.

**A component the log did not record is dropped and the rest renormalise** —
old logs without headshot data, or without a server log, are not scored zero
for it.

---

## 5. The models, class by class

Each class has a model of its own, built the same way: every decided match
gives a pair (Highlander guarantees one of each class a side), a logistic fit
says which components still predict the winner once the others are known, the
coefficients are rounded, and cross-validation over blocks of time scores the
method. Since v12 round impact is half of every model (the Medic's, under a
third) and the weight the other parts gave up went to kills and damage. The
accuracies of each fit, and the reasoning for every change, are in the comments
of the weights file. Shares are of the model on a KOTH map.

### Sniper

| | |
|---|---:|
| round_impact | 50% |
| impact_kills | 25% |
| deaths | 8% |
| untraded | 6% |
| dpm | 6% |
| untraded_deaths | 5% |

On payload and attack/defend maps deaths weigh 0.05 where they weigh 0.15 on KOTH and 5CP.

Round impact and kills carry him. A death on payload costs a third of a KOTH one (v13): on attack a Sniper is meant to take risks.

*Round impact 50% of the model, dying 12%, damage 6%.*

### Scout

| | |
|---|---:|
| round_impact | 50% |
| impact_kills | 14% |
| impact_assists | 9% |
| caps | 6% |
| deaths | 6% |
| dpm | 5% |
| medic_picks | 3% |
| untraded | 3% |
| untraded_deaths | 3% |

Assists nearly as heavy as kills: he arrives first, takes someone to half health and leaves the kill to the Soldier behind him. Caps because a Scout capping does the job nobody else does as fast.

*Round impact 50% of the model, dying 9%, damage 5%.*

### Soldier

| | |
|---|---:|
| round_impact | 50% |
| impact_kills | 23% |
| impact_assists | 10% |
| untraded_deaths | 6% |
| dpm | 6% |
| deaths | 5% |

The plainest model: nothing class-shaped survived the fit, and nothing was added to make it look fuller.

*Round impact 50% of the model, dying 11%, damage 6%.*

### Pyro

| | |
|---|---:|
| round_impact | 50% |
| impact_kills | 16% |
| impact_assists | 10% |
| deaths | 8% |
| dpm | 6% |
| medic_picks | 6% |
| untraded_deaths | 4% |

His assists outweigh his own kills: the spam and the airblast that make someone else's kill. A Pyro rated on kills and damage is rated on what he is worst at.

*Round impact 50% of the model, dying 12%, damage 6%.*

### Demoman

| | |
|---|---:|
| round_impact | 50% |
| impact_kills | 17% |
| deaths | 10% |
| impact_assists | 8% |
| dpm | 6% |
| untraded | 3% |
| untraded_deaths | 3% |
| fight_swing_shared | 3% |

The fight swing shared with whoever damaged the victim in the five seconds before: a Demo's kills usually finish what someone else's damage started.

*Round impact 50% of the model, dying 12%, damage 6%.*

### Heavy

| | |
|---|---:|
| round_impact | 50% |
| impact_kills | 21% |
| impact_assists | 10% |
| deaths | 8% |
| dpm | 6% |
| untraded_deaths | 4% |

The heaviest kill weight of the nine: a Heavy not killing people is a Heavy walking.

*Round impact 50% of the model, dying 12%, damage 6%.*

### Engineer

| | |
|---|---:|
| round_impact | 50% |
| impact_kills | 14% |
| impact_assists | 9% |
| deaths | 9% |
| fight_swing_shared | 6% |
| dpm | 5% |
| untraded | 3% |
| caps_contested | 3% |
| untraded_deaths | 1% |

Caps contested and the fight swing stand in for what a log cannot see: no sentry damage, no teleporter uses, no building placement. The biggest known weakness in the app.

*Round impact 50% of the model, dying 10%, damage 5%.*

### Medic

| | |
|---|---:|
| round_impact | 29% |
| heal | 25% |
| impact_assists | 14% |
| caps_contested | 11% |
| drops | 7% |
| deaths | 7% |
| untraded_deaths | 7% |

Round impact is under a third here (0.4 of 1.4) and not half: a Medic has few kills to add any, so it is mostly what his own deaths cost, which is dying under another name. With drops and dying, round impact is exactly half of his model (v14, a judgement and not a refit). Drops stay in on purpose: they are the first thing said about a Medic after a lost round.

*Round impact 29% of the model, dying 14%, damage 0%.*

### Spy

| | |
|---|---:|
| round_impact | 50% |
| impact_kills | 18% |
| impact_assists | 6% |
| medic_picks | 6% |
| untraded | 6% |
| fight_kast_engaged | 6% |
| untraded_deaths | 5% |
| dpm | 2% |

Kills not traded is the Spy's job stated correctly: a Spy who gets the Medic and dies for it has done the round's work. Who he killed matters, not how: backstabs are not rated, the victim already is.

*Round impact 50% of the model, dying 5%, damage 2%.*

---

## 6. Rules that apply to every model

**No model is mostly about dying.** Left alone the fit gave the Sniper 0.55
and the Medic 0.65 to death-shaped components. A rating that is mostly "who
died less" describes the team, not the player. A test holds all nine to at
most half, and counts a Medic's round impact as dying, because for him it is
(v14).

**Round impact is half of every model but the Medic's**, and deaths count
little outside it, so a player who takes more fights and wins them comes out
ahead even when they die more (Flashy, v12).

**Damage per minute is a small part (a tenth at most) of every model but the
Medic's.** The v9 refit found it redundant in all nine; v12 brought it back,
small, with the weight deaths gave up, and the before/after report picked the
next map's winner as well as v11 did.

**Opponent strength is corrected for in the season tables, not in a single
game's rating.** An official against a team at 1.10 (strength measured)
counts 0.075 more (one at 0.90, 0.075 less), the strength taken from each team's results
that season (Bradley-Terry) and not from how its players' logs rated. A
single match's rating is shown as it came out; the profile reports the
strength of the opposition beside it.

**Season ratings weigh an official by its minutes.** One rating per official,
playoffs counting 1.5 and the Grand Final 2.0, each by how much of a full
30-minute official it was (a six-minute substitute appearance is a fifth of
one), pulled towards 1.00 by three pseudo-officials. Award eligibility counts
the same units. The Season MVP's season term is the regular season alone,
because its blend already counts the final and the playoffs as terms of
their own.

---

## 7. What to attack

Points where a Highlander player's judgement beats the maths:

1. **Victim values** (§3) — the app cannot test these. Is a Medic really 3× an
   Engineer? Is a Spy really below a Heavy?
2. **The defending-Engineer 1.6** — proposed, never validated.
3. **Engineer and Pyro** are measured almost entirely by proxies. Are the
   proxies the right ones?
4. **The Medic's v14 weights** — a judgement, not a refit: round impact under
   a third, dying and round impact together half. Right call or sentimentality?
5. **Caps are weighted by what they cost.** Each capture counts the enemies
   who were alive to stop it, so walking onto a wiped point adds nothing and
   taking one into a full defence adds nine. It needs the raw server log, so
   the oldest logs that only exist on logs.tf lose the component rather than
   keep a flat count.
6. **Clean-up discount** only applies at 3+ up. Should being *down* players
   make a kill worth more?
7. **Everything is fitted against "did their team win"**, which is
   confounded: a player on a winning team dies less *because* they are
   winning. It is the only label this data has, and it is why the dying cap
   exists.

---

*Generated from model v14. Weights live in
`crates/hl-rating/src/weights.default.toml`.*
