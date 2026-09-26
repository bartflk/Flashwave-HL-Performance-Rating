# Flashwave.tf 0.4.0

Windows only. Download the `-setup.exe` and run it.

**Windows will say "Windows protected your PC"** — the installer is not
code-signed. Right-click the download → **Properties** → tick **Unblock** →
**OK**, and it runs with no warning at all. Check it is really my build
first:

```powershell
Get-FileHash .\Flashwave.tf_0.4.0_x64-setup.exe -Algorithm SHA256
```

The hash is at the bottom of this page. If it does not match, do not run it.

**Updating keeps everything.** Same data folder. The app re-rates itself on
first start, which takes a few seconds.

---

## Every class has its own rating model

Three classes had a model and the other six shared one called `generic` —
kills, damage, deaths, assists, caps, in proportions nobody had ever
checked. A Pyro was rated as though he were a Soldier with a worse gun.

All nine now have their own, each fitted against who actually won and
scored by cross-validation, so the numbers below are not the ones they were
tuned on:

| class | was | now | | class | was | now |
|---|---:|---:|---|---|---:|---:|
| Pyro | 73.5 | **78.0** | | Demoman | 72.0 | 75.6 |
| Heavy | 72.0 | **76.8** | | Medic | 71.1 | 75.4 |
| Sniper | 72.9 | 75.9 | | Scout | 70.7 | 74.5 |
| Engineer | 71.8 | 73.4 | | Soldier | 72.2 | 73.2 |
| Spy | 71.0 | 72.9 | | | | |

That is how often the higher-rated player's team actually won.

**Your off-class numbers will move, some a lot** — the old model flattered
classes whose job it could not see. Three findings worth knowing:

- **Damage per minute is in none of the nine.** Added to every model it
  changed accuracy by between −0.6 and +0.3 points: noise, both ways. Its
  value already reaches the rating twice, as the kills it sets up and the
  assists it becomes.
- **The Pyro is paid for assists, not kills.** His model weighs assists
  above his own kills, plus holding the point and being alive for the push.
  Biggest improvement of the nine.
- **The Medic keeps ubers and drops though the fit did not want them.**
  Both are collinear with dying, so dropping them scores 1.6 points better.
  A rating that cannot see a drop is not a Medic rating. They stayed, and
  the 1.6 points are the price.

No model gives dying more than half its weight — a rating that is mostly
"who died less" describes the team, not the player.

## Who you played

New on the profile. Your rating split by how good the opposite number on
your class is, averaged over *their* other games:

| | you | opponent | games |
|---|---:|---:|---:|
| Weaker opponents | 1.13 | 0.82 | 116 |
| An even match | 1.04 | 1.01 | 86 |
| Stronger opponents | 1.01 | 1.16 | 161 |

It is shown, not folded into the rating. Correcting for opponent strength
turns out to change nothing about how well the rating describes a player,
and would only hide how hard the night was.

## Look other people up

A **Players** tab. Search by name or Steam ID and see anyone who has played
in a match you have stored — their classes and rating on each, how often
they were on your team or against it, the head-to-head, and their best and
worst games, clickable through to the match.

Every figure is *in your matches*: this cannot see a game you were not in,
and does not pretend to.

## Themes

Four of them in Settings: **Gravel** (the one you have), **Dustbowl**,
**Coldfront**, **Swiftwater**. Team colours stay RED and BLU, and kills
stay blue against orange deaths — those mean something and a theme does not
get to repaint them.

## Fixed

- **Queued demo downloads never started** (Gilaric). There was no queue: a
  second download was refused while the window had already drawn a card for
  it, so it sat at "0 MB so far" until you cancelled and started it again.
  There is a real queue now, and the card says where it is in the line.
  Dismissing one that has not started cancels it.
- **Rating over time squashed after "Show as table" twice.** It measured an
  element the table view removes and never noticed when it came back.
- **"2 failed; next sync retries them", and nothing else** (KamikaZe).
  Settings now lists every log that would not download with its reason, and
  retries one or all. You can also paste a log id or logs.tf link to fetch
  it on the spot — for a match no index ever listed, which is where syncing
  again cannot help.
- **A map filter called itself a round.** Picking one map of a combined log
  said "in this round" under numbers covering seven of them.
- **Backups had advice you could not follow.** The panel said to keep a copy
  somewhere else and gave no way to do it; **Save a copy elsewhere…** now
  does.

## Smaller

- The **head-to-head bar** on the match page was still scaled for the old
  0–100 rating, so a real gap between two players drew a sliver and every
  matchup looked even. It now fills against a 0.5 gap.
- **Three maps were reading their own names wrong** and so had no overview
  image: `koth_product_rcx`, `pl_badwater_pro_v12` and `tow_tetsudo_b10a`.
  Product has an image, so that one had been drawing without it.
  (Most maps still without an overview are simply not in the placement
  table yet — that needs per-map calibration, not better matching.)
- **Fewer explanations.** A lot of panels described themselves at length to
  someone already looking at them. What is left is what you cannot infer.

## Also

Scout picks on KOTH were measured again properly, and the answer is the
opposite of the guess: on KOTH the Scout is the average class to kill, and
on stopwatch he is the *best* one. Nothing changed in the app — the numbers
are in `PLAN.md` under Q4.

---

**SHA-256**

```
0C4CA1FF0DCD398E51AF4BBA6A0D0C0FBB072A5A9BB72F6FDB946043A948E498  Flashwave.tf_0.4.0_x64-setup.exe
BA3AC7AB5B3503AF4AFFCFC5E4E362A91A378A046C59EED9AD92F79792EB7169  Flashwave.tf_0.4.0_x64_en-US.msi
```
