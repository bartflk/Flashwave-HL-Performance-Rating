# Flashwave.tf 0.3.1

Windows only. Download the `-setup.exe` and run it. Windows may say "Windows
protected your PC" because the installer is not code-signed: click **More
info**, then **Run anyway**.

**Updating from 0.3 keeps everything.** Same bundle id, same data folder.
The app re-rates itself on first start — see below, every class moves.

## Every class has its own rating model

Until now three classes had a model and the other six shared one called
`generic`: kills, damage, deaths, assists, caps, in proportions nobody had
ever checked. A Pyro was rated as though he were a Soldier with a worse gun.

Each of the nine now has its own, built the same way: every decided match
gives a pair — Highlander guarantees one of each class a side — a fit says
which components still predict the winner once the others are known, and
those get rounded into weights. Five-fold cross-validation over blocks of
time scores the method, so the numbers below are not the ones it was tuned
on.

| class | was | now | | class | was | now |
|---|---:|---:|---|---|---:|---:|
| Pyro | 73.5 | **78.0** | | Demoman | 72.0 | 75.6 |
| Heavy | 72.0 | **76.8** | | Medic | 71.1 | 75.4 |
| Sniper | 72.9 | 75.9 | | Scout | 70.7 | 74.5 |
| Engineer | 71.8 | 73.4 | | Soldier | 72.2 | 73.2 |
| Spy | 71.0 | 72.9 | | | | |

That is how often the higher-rated player's team actually won.

**Your off-class numbers will move, some of them a lot.** The old model
flattered classes whose job it could not see. Expect an Engineer or a Heavy
rating to drop and a Pyro to make more sense.

Three things worth knowing about what came out of it:

- **Damage per minute is in none of the nine.** Added to every model at 0.10
  it changed accuracy by between −0.6 and +0.3 points: noise, in both
  directions. Damage still matters — its value reaches the rating as the
  kills it sets up and the assists it becomes. It is counted twice already.
- **The Pyro is paid for assists, not kills.** His model puts more weight on
  assists than on his own kills, plus holding the point and being alive when
  the push comes. That is the largest single improvement of the nine.
- **The Medic keeps ubers and drops even though the fit did not want them.**
  They are collinear with dying — a Medic who dies does not build, and a drop
  is a death with an uber in hand — so the fit drops both and scores 1.6
  points better. Drops are the first thing said about a Medic after a lost
  round. A rating that cannot see one is not a Medic rating, so they stayed
  and the 1.6 points are the price.

No model gives dying more than half its weight. Left alone the fit gave the
Sniper 0.55 and the Medic 0.65, and a rating that is mostly "who died less"
describes the team rather than the player.

The model is **v7**. Your v6 numbers are left in the database untouched.

The reasoning for all nine is written beside the weights in
`weights.default.toml`, inside the app's folder — including what was tried
and rejected.

## Fixed, from your reports

**Queued demo downloads never started** (Gilaric). There was no queue. A
second download was refused outright while the window had already drawn a
card for it, so the card sat at "0 MB so far" until you cancelled and
started it again. There is a real queue now: one at a time in the order you
asked, and the card says where it is — "next, once the one before it
finishes". Dismissing one that has not started cancels it, which it also
used to not do.

**Rating over time squashed after "Show as table" twice.** The chart was
measuring an element the table view removes, and never noticed when it came
back. It is measured properly now, and scales instead of stubbing if it ever
gets it wrong.

**"2 failed; next sync retries them", and nothing else** (KamikaZe).
Settings has a new panel, **Logs that didn't import**: every log that would
not download, with its map, its logs.tf link, the error and how many times
it has been tried. Retry one or all of them, or paste a log id or logs.tf
link to fetch it there and then — for a match no index ever listed, or one
logs.tf holds under a second id, which is where syncing again cannot help.

## Also

Scout picks on KOTH were measured again, properly this time, and the answer
is the opposite of the guess: on KOTH the Scout is the average class to
kill, and on stopwatch he is the *best* one. Nothing changed in the app —
the write-up is in `PLAN.md` under Q4 if you want the numbers.
