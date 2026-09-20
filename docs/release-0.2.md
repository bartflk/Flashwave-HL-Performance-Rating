# HL Rating 0.2

Windows only. Download the `-setup.exe` from the release and run it. Windows
may say "Windows protected your PC" because the installer is not code-signed:
click **More info**, then **Run anyway**.

**Updating from 0.1 keeps everything.** Install over the top; your logs, demo
index and ratings stay where they are, and the new tables are added on first
start. Going back to 0.1 afterwards does not work — the older build does not
know the new tables — so restore a backup if you ever need to.

## Your aim, read from your own demos

The app now reads the demos in your TF2 folder properly, not just their
headers. For every kill it can see, it measures:

- **where your crosshair was** when the kill landed, and one second earlier;
- **the flick**: how far your view turned in the last half second;
- **the range and height** of the shot, exactly;
- and for every death: **who killed you, from how far, how close your nearest
  teammate was**, and whether you were scoped.

The match page has a new **Aim** tab built around a target: the centre is the
head you killed, each dot is where your crosshair sat, and the bullseye is the
angle a head really covers at your usual range. A cross marks where your
crosshair normally sits, which says more than any single kill. One bar splits
your kills into angles you already held, small adjustments and flicks; another
picture shows how close your nearest teammate was when you died.

The Profile page carries the same numbers for a season, a kind of game, or a
class, each against your usual figure.

Reading a demo takes about four seconds, and it happens after a sync for any
demo newly linked to a match. Nothing leaves your machine.

## Ratings: kills now count for what the situation is worth

The Sniper model moves to **v5**. A kill while three or four players up counts
for less, because cleaning up a fight already won is not what decides rounds.
That comes from measuring 193,626 kills: a kill at four up lifts the chance of
winning the round by a fifth as much as one at even numbers.

Against 693 matches with a Sniper on each side, the model picks the winning
side 73.6% of the time, and 72.8% on matches it has never seen, against 73.2%
and 72.3% in 0.1. Small, and honestly reported.

## The match page

- **Class matchups** open into a mirrored comparison: score cards, a line
  naming what decided it, and a bar per component showing both players'
  percentiles and how many rating points each row swung.
- **Combined logs** list the individual logs they were built from, each
  linking to logs.tf. They are still counted once, as before.
- **The scoreboard** tints rows by team instead of striping them.
- **The kill map** has a full-screen button.
- **Round and map filters now reach every tab.** Fights and Damage-and-kills
  used to show the whole match whatever you picked. Three columns still need
  the whole match — Fight KAST, forces, and deaths around an uber — and say so
  rather than showing match totals under a round heading.

## The match list

Sort by kills, damage, DPM or your rating by clicking a heading, and a Rating
column shows what you were worth in each match. Sorting runs over your whole
history, not just the rows on screen.

## Your data

A copy of the database is made before every sync and rebuild: the newest five
live in a `backups` folder beside it, and Settings lists them with a **Back up
now** button. To restore one, close the app and rename the copy over
`hl.sqlite3`.

The uninstaller offers a **"Delete the application data"** checkbox. It is
unticked by default, and updating never removes anything, but if you tick it
during an uninstall it takes the backups with everything else. Keep a copy
elsewhere if your history matters to you.

## Thanks

To function, boSe and Taiga for testing 0.1 and saying what was wrong with it.
The filters bug, the combined-log list, the scoreboard colours and the target
view all came from that feedback.
