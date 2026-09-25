# Highlander, and the parts of TF2 a rating has to know

The rules and mechanics this app is built against, with the numbers, so that a
model can be reasoned about rather than guessed at. Everything here is either
a league rule, a wiki-documented game mechanic, or a claim from competitive
players — and the three are labelled, because they are not the same kind of
thing. Sources at the bottom.

Read `PLAN.md` §11 for what the community says *wins* games and how each claim
could be tested. This file is the ground underneath that: what the game
actually is.

---

## 1. The format

**Nine players a side, one of each class.** That is the whole idea: a class
limit of 1, so every team fields exactly one Scout, Soldier, Pyro, Demoman,
Heavy, Engineer, Medic, Sniper and Spy. A second of any class outside the
spawn room is a rule break — RGL makes it an immediate round loss.

- **ETF2L**: rosters need nine players; a match needs at least eight, and 8v9
  is playable. Fewer than eight is a default loss. Up to **3 mercenaries** per
  map (2 in Grand Finals), and nobody may merc for the same team more than
  twice.
- **RGL**: same 9v9 with the option to play 8v9.

**Weapons are whitelisted, not free.** Both leagues run an item whitelist, and
RGL auto-bans anything a TF2 update adds until admins rule on it. This matters
for the app only in that a log's weapon names are drawn from a restricted set,
and an unusual one is worth noticing rather than trusting.

---

## 2. Maps and how they are won

Three game modes appear in Highlander. They are scored differently, which is
the single most important thing for a rating to get right — a player can have
a huge game on a map their team loses on the clock.

### Stopwatch (Payload, and Attack/Defend CP)

Played **ABBA**: each team attacks once, then the sides swap. A round is one
team's attack. The attacker who gets further, or gets the same distance
faster, wins the round.

- Best of three rounds; first to two wins the map.
- **No time limit** on the map itself.
- A 2–1 result is a **Golden Cap** (ETF2L), worth fewer league points than a
  clean 2–0.
- RGL: rounds 1 and 2 are the two sides; round 3, if needed at 1–1, has the
  first team choose its side.

**The consequence for a rating:** stopwatch is decided by *time*, not by
frags or by rounds won. A defence's output is seconds held per point; an
attack's is seconds taken per point. A team can lose most of the fights and
win the map.

### King of the Hill

One point in the middle. Each team has a clock; holding the point runs it
down. **Three minutes of held time wins a round.**

- **ETF2L**: first to **3 rounds** wins the map, no time limit. A 3–2 is a
  Golden Cap.
- **RGL**: first to **4 rounds**, played in at least two halves.

The mid-fight opens every round and usually decides it, though not always.

### 5CP

Rarely in the modern Highlander pool but still in the rules: 30-minute limit,
win by 5 rounds' difference; a tie goes to a 15-minute Golden Cap, first round
wins.

### The pool moves every season

Season 36 (Autumn 2026), ETF2L: **pl_vigil_rc10, cp_steel_f12, pl_upward_f12,
koth_product_final, koth_proot_b5b**. `koth_warmtic_f10` and
`koth_ashville_final1` were dropped after S35.

Maps carry version suffixes that change without the map changing meaningfully
(`pl_upward_f12`, `pl_upward_rc7`). The app strips them — see
`hl-core::map_base` — because "my Upward games" is one thing to a player.

---

## 3. The nine classes

Base health and speed, from the wiki. Speed is given as a percentage of the
300 HU/s baseline and in Hammer units per second, because demo parsing
measures distance in those units.

| Class | Role | Health | Speed | In Highlander |
|---|---|---:|---:|---|
| Scout | offense | 125 | 133% · 400 | Flank. Fast enough to reach a Sniper or a Medic that strays. Counts as **two people** pushing a cart. |
| Soldier | offense | 200 | 80% · 240 | Flank or combo. Rocket jumps to angles nobody else reaches. |
| Pyro | offense | 175 | 100% · 300 | Combo's bodyguard: airblast, Spy-checking, denying an Über push. |
| Demoman | defense | 175 | 93% · 280 | The combo's damage. Losing him usually ends a push. |
| Heavy | defense | 300 | 77% · 230 | The combo's body. Takes the space the Medic heals him through. |
| Engineer | defense | 125 | 100% · 300 | Anchors a defence; teleporters and a mini on attack. Worth the *seconds* he buys, not the kills. |
| Medic | support | 150 | 107% · 320 | The game. His Über is the resource both teams play around. |
| Sniper | support | 125 | 100% · 300 | Picks, and first the duel with the other Sniper. |
| Spy | support | 125 | 107% · 320 | Picks from behind, especially the Medic. |

**The four groups.** Teams do not play as nine individuals:

- **Combo** — Medic, Heavy, Demoman, usually Pyro. Takes and holds space.
- **Flank** — Scout and Soldier. Off-angles, clean-up, pressure on the
  backline.
- **Picks** — Sniper and Spy. Create openings by removing a key player before
  or during a fight.
- **Engineer** — buys time on defence; much less on attack.

---

## 4. The mechanics that decide fights

### Übercharge

The resource the whole game is built around.

- Stock Medi Gun builds at **2.5%/s → 40 seconds** for a full charge. **Triple
  rate during setup**, so about 13.3 seconds.
- An Über lasts **9 seconds** of invulnerability.
- **Heal rate**: 24 HP/s on a recently damaged patient, rising linearly to
  **72 HP/s** once they have been undamaged for 15 seconds.
- **Overheal caps at 150% of base health** — a buffed Heavy is 450, a buffed
  Medic 225.

Three derived ideas the app already uses or should:

- **Über advantage** — one team has a charge and the other does not. The
  holder can push without fear of a mirrored charge.
- **Über force** — making a Medic spend a charge to survive, winning nothing
  with it. A pick's value is often that it forces this.
- **Dry push** — pushing with no charge. Usually only correct with numbers.

### Picks, numbers and trades

- A **pick** is a kill that opens the possibility of a push (RGL's glossary).
  Its value is entirely in what happens next.
- A **traded** pick — where the picker's team immediately loses someone back —
  opens nothing. This is why the model counts untraded kills and untraded
  deaths separately.
- **Numbers** is simply who has more players alive. The app measures the
  win-chance lift per numbers state; see `hl situation`.

### What a log actually records

logs.tf gives per-player totals and, in the raw server log, every kill with
its timestamp, both classes, both positions, and the weapon. It does **not**
record: who was healed at the moment of a kill, Über percentage over time,
where a player was when they were not killing or dying, or cart position.
Those come from a demo, or not at all.

---

## 5. The cart

Relevant in detail because two queued features measure it.

- Pushing requires standing next to it. Speed scales to a cap of **three
  pushers**: 1 pusher 50 HU/s, 2 pushers 70 HU/s, 3+ pushers 90 HU/s.
- A **Scout counts as two** pushers.
- **A single enemy near the cart blocks it entirely** — it does not slow, it
  stops, until they leave or die.
- Unattended, it **rolls back after 30 seconds**, and immediately on marked
  rollback hills.
- It acts as a **dispenser** for the attacking team, with unlimited metal.
- Checkpoints add time to the clock. Overtime gives about five seconds of
  grace while the cart is moving.

The blocking rule is why "cart time" is a real measurement and not a proxy:
a stationary cart in a 9v5 is not a resourcing problem, it is a mistake.

---

## 6. What this means for a rating

Consequences worth keeping in front of the model:

1. **Mode changes what output means.** Seconds are the currency on stopwatch;
   rounds are the currency on KOTH. A single rating pooled across both hides
   this — see PLAN §12 step 4 (map and side baselines).
2. **Side changes it too.** Defending Upward and attacking Upward are
   different games with different expected numbers.
3. **Class changes everything.** A Medic's output is heals, Übers and drops; a
   Sniper's is picks and the duel. Nine models, not one — done in Q8, and
   each one's reasoning is written beside its weights in
   `crates/hl-rating/src/weights.default.toml`. Two of them are worth reading
   as a check on this document: the Pyro's model puts more weight on assists
   than on his own kills, and the Engineer's leans on caps, because in both
   cases the log can only see the job indirectly.
4. **A kill is not a kill.** Value depends on the victim's class, the map, the
   side, and the state of the fight. This is the v5 situation work.
5. **Damage flatters spam.** Damage into a choke builds the enemy's Über. Net
   frags per minute tracks outcome better than K/D — and when Q8 fitted all
   nine models, damage per minute failed to earn a place in any of them. Not
   because damage does not matter, but because the value of damage arrives
   twice already: as the kills it sets up, and as the assists it becomes.
6. **The log is not the game.** Everything above the log's totals — position,
   timing, who was with whom — needs the demo.

---

## Sources

League rules:

- [ETF2L 9v9 rules](https://etf2l.org/9v9-rules/) — class limits, team size,
  match formats, golden cap, mercenaries.
- [ETF2L Highlander Season 36 announcement](https://etf2l.org/2026/07/23/announcing-highlander-season-36-autumn-2026/) — the current map pool.
- [RGL Highlander format rules \[2001\]](https://docs.rgl.gg/rules/hl/2001/) —
  class limit enforcement, stopwatch and KOTH formats.
- [RGL glossary](https://docs.rgl.gg/guides/basics/glossary/) — "pick" and the
  rest of the vocabulary.

Game mechanics:

- [TF2 wiki: Classes](https://wiki.teamfortress.com/wiki/Classes) — health and
  speed.
- [TF2 wiki: Medi Gun](https://wiki.teamfortress.com/wiki/Medi_Gun) — charge
  rate, Über duration, heal rates, overheal cap.
- [TF2 wiki: Payload](https://wiki.teamfortress.com/wiki/Payload) — cart speed,
  blocking, rollback.

Competitive theory (claims, not facts — see PLAN §11 for how each is tested):

- [TF2 wiki: Highlander (competitive)](https://wiki.teamfortress.com/wiki/Highlander_(Competitive))
- [TF2 wiki: Sniper (competitive)](https://wiki.teamfortress.com/wiki/Sniper_(competitive))
- [TF2 wiki: Competitive dynamics](https://wiki.teamfortress.com/wiki/Competitive_dynamics)
- [An introduction to European Highlander](https://steamcommunity.com/sharedfiles/filedetails/?id=163882605)
- [teamfortress.tv: TF2's hidden stats](https://www.teamfortress.tv/41723/tf2s-hidden-stats-part-1)

**Checked September 2026.** League rules change every season: the map pool
especially, and occasionally the KOTH round count. Re-check before building
anything that depends on a specific number.
