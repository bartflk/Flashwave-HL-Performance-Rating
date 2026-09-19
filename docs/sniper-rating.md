# HL Sniper Rating: What Goes In
*As of 19 Sep 2026*

## How it works
A Sniper rating is **0–100**, where **50 = a typical game by the Snipers you play against**. It's built from nine stats, each scored against every other Sniper game in the stored matches.

1. **Each stat becomes a percentile.** 70 on DPM = more DPM than 70% of Sniper games against you. For stats where lower is better (deaths) it's flipped, so higher is always better.
2. **The rating is a weighted average of those percentiles.**
3. **Missing stats are skipped, not scored as zero.** Old logs without a raw server log have no fight stats; the other weights fill in.

The pool is ~1,500 Sniper games from 753 matches since 2014. The weights are checked against who actually won (see *How we check it*).

## What goes into the Sniper rating now (v4)
Nine stats. **60% output** (kills, damage, deaths), **40% impact** (what your kills did). Same balance HLTV uses for CS.

- **Kills in context: 20%.** Kills per 10 min, each worth its victim's value (Medic 3.0, Demo 2.2, Sniper 1.8…). *Why: the job is picks on the right targets. A kill while three or four players up counts less: cleaning up a lost fight is not what decides rounds.*
- **Damage / min: 20%.** *Why: pressure and chip damage that kills alone miss.*
- **Untraded deaths: 15%.** Deaths your team didn't avenge within 3 s. *Why: a dead Sniper holds nothing, but a traded death still opened a fight.*
- **Opening duels: 10%.** First kills of fights got, minus first deaths of fights. *Why: opening the fight is the Sniper's job.*
- **Medic picks: 10%.** *Why: the single most important kill.*
- **Fight KAST: 10%.** % of fights you were alive for where you got a kill or assist, survived, or were traded. *Why: useful every fight, not just in a few big ones.*
- **Deaths to flankers: 5%.** Deaths to a Scout, Spy or Soldier. *Why: being caught from the side or behind.*
- **Kills not traded: 5%.** % of your kills where your team didn't lose someone within 3 s. *Why: a kill traded straight back opened nothing.*
- **Sniper duel: 5%.** Kills on their Sniper minus deaths to them. *Why: winning the duel frees your picks, but a Spy can kill their Sniper too.*

Other classes use simpler weights for now; the Sniper is the focus.

## How the values are counted
Everything comes from the logs.tf raw server log: every kill with both players' positions, every hit, every uber. No demos needed.

**Kill values by victim class:**
- Medic 3.0, Demoman 2.2, Sniper 1.8, Pyro 1.5, Heavy 1.4
- Spy 1.3, Soldier 1.2, Scout 1.15 (1.0 defending), Engineer 1.1 (1.6 defending)

"Defending" = RED on payload and attack/defence maps. Pyro and Spy were raised on a Premiership Sniper's advice.

**The two timing rules** (measured on 195,000 kills):
- **A fight** = kills with no gap over **10 s** (89% of gaps between kills are 10 s or less).
- **A trade** = the other team kills back within **3 s** (kills right after a kill peak at 1 s and halve by 3 s).

**Terms:**
- **Opening kill/death:** the first kill of a fight.
- **Traded death:** you died, and your team killed someone on the killer's team within 3 s.
- **Flankers:** Scout, Spy, Soldier. **Combo:** Medic, Demo, Heavy, Pyro.

## Tried and left out
Tested against who won, then dropped or cut. Still shown on the match page and profile for reference.

- **Headshot share: 10% → 0%.** The Sniper with the higher share was on the losing team slightly more often (47% won).
- **Sniper duel: 20% → 5%.** Once kills and deaths are counted, winning the duel adds nothing; bad forced peeks (Vigil 2nd, the hill) punished it.
- **All deaths: 15% → 0%.** Replaced by untraded deaths: a traded entry death isn't a failure.
- **Stayed put and died:** tested, left out. Dying near a spot you already got two kills from predicts nothing (46%).
- **Fight KAST only after a shot:** tested, left out. Counting survival only if you fired did worse than counting it always.
- **DPM: 5% → 20%.** Raised on the players' view; the data says it's safe but mostly overlaps with kills.

## How we check it
In **693 decided matches** with a rated Sniper on each team: how often did the higher-rated Sniper's team win? Also scored on matches since Season 34 alone, so a version can't look good just by fitting older games.

- **v1** (duel 20%, headshots 10%, DPM 5%): 67.5% (66.8% since S34)
- **v2** (DPM 20%, duel 5%, headshots 0%, opening duels): 71.4% (69.3%)
- **v3** (untraded deaths, deaths to flankers): 72.4% (71.3%)
- **v4, live** (Fight KAST): **73.2% (72.3%)**

A match is decided by nine players, so no Sniper rating picks every winner. A model fitted purely on the data reaches ~75%, roughly the ceiling for these stats.

**Example:** S33 Low grand final (logs.tf/3863290). v1 rated angel complex 56.5 and flashy 50.6, though both got 110 kills and flashy won the opening duels 22–5. v4: **flashy 62.3, angel complex 59.2.**

## Coming next
Each only goes in if it picks winners at least as well on matches it wasn't tuned on.

- **Kills valued by the situation.** A kill at even numbers, or on a team with uber ready, is worth more; a clean-up in a 9 v 5 is worth less. *Why: 39% of Sniper kills are clean-ups (like HLTV devaluing kills on players with bad guns).*
- **Map and side comparison.** You're compared with Snipers on the same map and side (e.g. defending Vigil). *Why: some Sniper jobs are harder; a hard Vigil game shouldn't look like a bad game.*
- **Fight swing.** How much each kill changed the chance of winning that fight, capped at ~15%. *Why: the purest "did this kill matter"; HLTV found it too swingy above a third.*
- **Opponent strength.** Games against better divisions count for more.

## Proposed: a Teamfights section
A team view of every fight in a match: who won it, how the ubers went, who the difference makers were. **Not in the rating: it's for reviewing games.** Built on Taiga's feedback.

**Per fight, one row:**
- When and where it happened (the spot where its kills cluster)
- Players alive and uber state when it started: who had advantage, who was ready
- Ubers popped: by whom, medigun, length in seconds, and who got it
- Kills each way, and players each team had left after
- Result: won, lost or even, and whether the next point was taken

**Team stats over a match:**
- **Fights won / lost.** The basic scoreboard of fights. ✅
- **Uber exchanges won.** Both teams popped within a few seconds: who lost fewer players during it, and who won the fight after. ✅
- **Collapsed on by an ad uber.** Fights where the enemy pushed with uber advantage, and how many you lost. ✅
- **Dying at bad times.** Deaths while your own uber was ready, or right before your team popped. ✅
- **Dropping players off cooldown.** Lone deaths outside any fight, e.g. walking back in alone after respawning. ✅
- **Kills in exchanges.** Kills where the killer also died in the same trade. ✅
- **Difference makers.** Players whose kills most often decided a fight. ⏳ after fight swing
- **Who got the uber.** The player the Medic ubered. 🟡 likely, from the Medic's heal target during the uber

✅ = the logs have what's needed · 🟡 = probably · ⏳ = needs another step first

## Feedback wanted
1. Are the kill values right? Is a Sniper worth 1.8 and a Pyro 1.5?
2. Is 3 s the right window for a trade?
3. A clean-up kill in a 9 v 5: worth half a kill, or close to a full one?
4. Which maps make the Sniper most and least important?
5. Teamfights: which of the team stats would you actually use when reviewing a game?
6. "Dropping players off cooldown": lone deaths between fights, or something else?
