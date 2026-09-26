# How the Flashwave.tf rating works

**Model v7. Written to be argued with** — if something here is wrong about
how Highlander is played, it is wrong in the app too, and the numbers in it
are the place to say so.

Everything below is generated from the app's actual weights file
(`crates/hl-rating/src/weights.default.toml`), not from memory.

---

## 1. The short version

A rating is **one player, on one class, in one match**.

```
for each component:   percentile against everyone else who played that class
rating (0–100 scale): weighted average of those percentiles
rating (what is shown): 1.00 + (score − pool mean) / pool sd × 0.25
```

- **1.00** is an average game.
- **One standard deviation is 0.25**, so 1.25 is a very good game and 0.75 a
  poor one.
- Floored at 0, nothing caps the top. Best real single games land near 1.70.

This is HLTV's idea: express a performance as how many standard deviations
it is from average, and centre that on 1.00.

## 2. What it is measured against

**The pool is the players you have actually faced.** Every stored Highlander
log holds 17 other players, so each class already has ~1,500 performances to
compare against without crawling anything.

- **You are excluded from your own pool.** 663 of ~1,500 Sniper
  performances in the development database are the owner's; rating him
  against a pool that is 44% himself would drag his median to average by
  construction.
- **Per map where there is enough data.** A pool needs **100+ performances**
  on that map for that class, otherwise it falls back to all maps. Measured:
  Vigil averaged 0.90 and Product 1.10 before this, three quarters of a
  deviation that belonged to the map and not the player.
- **Minimum 5 minutes** on the class, or it is not rated at all — a spawn
  swap is not a performance.
- **Main class only.** logs.tf records kills per player, not per class
  played, so a flexer's kills cannot be split honestly between classes.

**Known limitation:** the pool is whoever you play. A Prem player and a
low-open player are not rated against the same standard, and the app says so
rather than pretending otherwise.

## 3. What a "kill" is worth

Not all kills count the same. A kill is valued by **who died**:

| victim | worth | why |
|---|---:|---|
| Medic | **3.0** | a Medic pick decides the fight |
| Demoman | 2.2 | the team's damage |
| Sniper | 1.8 | denying their picks |
| Pyro | 1.5 | with their Pyro dead you can spam freely |
| Heavy | 1.4 | |
| Spy | 1.3 | important, rarely decides a teamfight |
| Soldier | 1.2 | |
| Scout | 1.15 | |
| Engineer | 1.1 | |

On attack/defence maps (every payload map, plus Steel, Gravelpit, Dustbowl,
Egypt, Gorge, Junction, Mountain Lab) killing a **defending Engineer is 1.6**
and a **defending Scout 1.0** — the sentry holds the defence, and the Scout
matters less defending than attacking.

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

## 4. The components

| component | what it measures | unit |
|---|---|---|
| `impact_kills` | kills, each valued by victim class/map/side | per 10 min |
| `situation_kills` | the same, also scaled by the clean-up discount | per 10 min |
| `impact_assists` | assists, valued the same way, at 0.5 | per 10 min |
| `medic_picks` | enemy Medics killed | per 10 min |
| `deaths` | deaths (**lower is better**) | per 10 min |
| `untraded_deaths` | deaths your team did **not** trade within 3 s (lower better) | per 10 min |
| `flank_deaths` | deaths to a Scout, Spy or Soldier (lower better) | per 10 min |
| `untraded` | share of *your* kills the enemy did not trade back | % of kills |
| `opening` | first kills of a fight, minus first deaths | net per 10 min |
| `fight_kast_engaged` | share of fights with a kill, assist, survival or traded death | % of fights |
| `caps` | points/cart captured | per 10 min |
| `heal`, `ubers`, `drops` | Medic's own numbers (drops lower better) | per min / per 10 min |
| `backstabs` | Spy | per 10 min |
| `duel` | kills on the enemy Sniper minus deaths to them | per 10 min |
| `headshot_share` | share of kills that were headshots | % |
| `dpm` | damage per minute | per min |

A fight = 10 s with nobody dying. A trade = a kill back within 3 s. Both
windows were measured on ~195,000 kills, not guessed.

**A component the log did not record is dropped and the rest renormalise** —
old logs without headshot data are not scored zero for it.

---

## 5. The models, class by class

Each was built the same way: every decided match gives a pair (Highlander
guarantees one of each class a side), a logistic fit says which components
still predict the winner once the others are known, the coefficients are
rounded to 0.05, and **five-fold cross-validation over blocks of time** scores
the method. The accuracy quoted is how often the higher-rated player's team
actually won, on data the model was not fitted to.

### Sniper — 75.9% (was 72.9%)

| | |
|---|---:|
| impact_kills | 0.25 |
| situation_kills | 0.20 |
| deaths | 0.25 |
| untraded_deaths | 0.15 |
| untraded | 0.10 |
| flank_deaths | 0.05 |

**Why:** what his picks were worth, and how often he was the one picked. The
duel, headshot share, opening duels and Fight KAST were all *removed* by the
fit — not because they say nothing (Fight KAST alone picks 72.4% of winners)
but because deaths, untraded deaths and valued kills already contain them.

### Scout — 74.5% (was 70.7%)

| | |
|---|---:|
| impact_kills | 0.20 |
| impact_assists | 0.20 |
| situation_kills | 0.10 |
| deaths | 0.20 |
| untraded_deaths | 0.15 |
| medic_picks | 0.05 |
| caps | 0.05 |
| untraded | 0.05 |

**Why:** assists as heavy as kills — he arrives first, takes someone to half
health and leaves the kill to the Soldier behind him. Caps because a Scout
capping is doing the job nobody else can do twice as fast.

### Soldier — 73.2% (was 72.2%)

| | |
|---|---:|
| impact_kills | 0.35 |
| untraded_deaths | 0.25 |
| impact_assists | 0.20 |
| deaths | 0.20 |

**Why:** the smallest gain of the nine and the plainest model. Nothing
class-shaped survived the fit, and nothing was added to make it look fuller.

### Pyro — 78.0% (was 73.5%)

| | |
|---|---:|
| impact_assists | 0.20 |
| deaths | 0.20 |
| untraded_deaths | 0.20 |
| impact_kills | 0.15 |
| caps | 0.15 |
| medic_picks | 0.05 |
| situation_kills | 0.05 |

**Why:** the largest improvement, and the clearest case for per-class models
existing. A Pyro rated on kills and damage is rated on the two things he is
worst at. **His assists outweigh his own kills** — the spam and the airblast
that make someone else's kill — plus holding the point and being alive when
the push comes.

### Demoman — 75.6% (was 72.0%)

| | |
|---|---:|
| deaths | 0.30 |
| impact_kills | 0.15 |
| fight_kast_engaged | 0.15 |
| medic_picks | 0.10 |
| untraded_deaths | 0.10 |
| flank_deaths | 0.10 |
| impact_assists | 0.05 |
| untraded | 0.05 |

**Why:** Fight KAST is his alone among the nine — he is in every fight and is
measured by whether it went his team's way, not by whether he got the kill.
Deaths to flankers because a Demoman who dies to the flank leaves his team
with no damage for ten seconds.

### Heavy — 76.8% (was 72.0%)

| | |
|---|---:|
| impact_kills | 0.30 |
| untraded_deaths | 0.25 |
| medic_picks | 0.10 |
| flank_deaths | 0.10 |
| fight_kast_engaged | 0.10 |
| situation_kills | 0.10 |
| untraded | 0.05 |

**Why:** the heaviest kill weight of the nine — a Heavy not killing people is
a Heavy walking. Medic picks because he is the class that can hold an angle
on their Medic and win it. Deaths to flankers weigh more because a dead
Heavy takes half a minute to matter again.

### Engineer — 73.4% (was 71.8%)

| | |
|---|---:|
| impact_kills | 0.20 |
| impact_assists | 0.20 |
| caps | 0.20 |
| untraded_deaths | 0.20 |
| deaths | 0.10 |
| opening | 0.05 |
| untraded | 0.05 |

**Why:** caps at 0.20 is the highest of the nine, and it is not really about
capping — it is the only thing in a log that knows his buildings were where
the team needed them.

**Biggest known weakness in the whole app.** logs.tf records no sentry
damage, no teleporter uses, no building placement. This is the best model
available from what a log says, which is not the same as a good measure of
an Engineer.

### Medic — 75.4% (was 71.1%, and 62.6% on the old generic model)

| | |
|---|---:|
| deaths | 0.25 |
| heal | 0.15 |
| ubers | 0.15 |
| untraded_deaths | 0.15 |
| drops | 0.10 |
| caps | 0.10 |
| impact_assists | 0.10 |

**Why, and this one overrules the data on purpose.** Left alone the fit gives
deaths 0.45, untraded deaths 0.20, and **drops ubers and drops entirely** —
both are collinear with dying, since a Medic who dies does not build and a
drop is a death with an uber in hand. That model scores **1.6 points better**.

It was rejected. Drops are the first thing said about a Medic after a lost
round, and a rating that cannot see one is not a Medic rating. The 1.6
points are the stated price.

### Spy — 72.9% (was 71.0%)

| | |
|---|---:|
| impact_kills | 0.20 |
| untraded_deaths | 0.20 |
| untraded | 0.15 |
| backstabs | 0.10 |
| duel | 0.10 |
| impact_assists | 0.10 |
| fight_kast_engaged | 0.10 |
| headshot_share | 0.05 |

**Why:** "kills not traded" is the Spy's job stated correctly — a Spy who
gets the Medic and dies for it has done the round's work; one who gets a
Soldier and is traded immediately has done nothing.

**The old hand-written Spy model scored worse than the generic fallback**
(71.0 against 72.3). Only pairing it against the fallback showed that.

---

## 6. Rules that apply to every model

**No model gives dying more than half its weight.** Left alone the fit gave
the Sniper 0.55 and the Medic 0.65 to death-shaped components. A rating that
is mostly "who died less" describes the team, not the player. A test holds
all nine to this.

**Damage per minute is in none of the nine.** Added to every model at 0.10 it
moved accuracy between −0.6 and +0.3 points — noise in both directions. Not
because damage does not matter, but because its value reaches the rating
twice already: as the kills it sets up (`impact_kills`) and the ones it hands
a teammate (`impact_assists`). This has now been measured twice.

**Opponent strength is shown, not corrected for.** Facing an opponent 0.20
better costs 0.076 rating points, but correcting for it changes how well a
player's games predict each other by 0.000 — opponent strength varies more
*within* one player's games than *between* players, so there is no standing
difficulty to subtract. The profile reports it instead.

---

## 7. What to attack

Points where a Highlander player's judgement beats the maths:

1. **Victim values** (§3) — the app cannot test these. Is a Medic really 3× an
   Engineer? Is a Spy really below a Heavy?
2. **The defending-Engineer 1.6** — proposed, never validated.
3. **Engineer and Pyro** are measured almost entirely by proxies. Are the
   proxies the right ones?
4. **Caps are not weighted by what they cost.** Sitting on the cart after a
   wipe currently counts the same as capping into a live defence. Known, and
   queued.
5. **Clean-up discount** only applies at 3+ up. Should being *down* players
   make a kill worth more?
6. **Everything is fitted against "did their team win"**, which is
   confounded: a player on a winning team dies less *because* they are
   winning. It is the only label this data has, and it is why the survival
   cap exists.
7. **Medic's 1.6-point trade** (§5) — right call or sentimentality?

---

*Generated from model v7. Weights live in
`crates/hl-rating/src/weights.default.toml`, which carries the same
reasoning inline and can be edited without rebuilding the app.*
