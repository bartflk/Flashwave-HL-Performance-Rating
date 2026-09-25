# Flashwave.tf 0.3

Windows only. Download the `-setup.exe` from the release and run it. Windows
may say "Windows protected your PC" because the installer is not code-signed:
click **More info**, then **Run anyway**.

**Updating from 0.2 keeps everything.** The app is called Flashwave.tf now
rather than HL Rating, but its data folder is named after the bundle id, which
has not changed — your logs, demo index and ratings stay exactly where they
are. The new installer lands under the new name beside the old Start menu
entry; uninstall the old one afterwards and leave "Delete the application
data" unticked.

**Ratings are renumbered.** Everything you had rated 0-100 now reads as an
HLTV-style rating around 1.00, and the app re-rates itself on first start.

## The rating is an HLTV rating

HLTV counts how many standard deviations a player is above or below average
and centres that on 1.00. This does the same. The components underneath are
still percentiles against the players you actually face — that is the working,
and the breakdown still shows it — but the number on top is now:

| | |
|---|---|
| **1.00** | an average game |
| **1.20+** | elite over a career |
| **1.40** | a strong game, about the top 5% |
| **0.60** | a poor one, about the bottom 5% |

One standard deviation is worth 0.25, picked so the numbers mean what they
mean on HLTV. Measured against this database's 13,568 rated performances, the
best regulars average 1.24 over a career, against HLTV's ZywOo 1.27 and
s1mple 1.23.

The rating model is **v6**. Your old v5 numbers are a different unit and are
left in the database untouched.

## Syncing does less, and says more about it

**It no longer downloads everything you have ever played.** A sync keeps the
last two years plus **every official at any age** — a season from 2019 is
still a game you care about — and leaves older scrims and pugs indexed but not
downloaded. On this account that is 476 matches fetched instead of 754, and a
first sync roughly 40% shorter. Nothing is deleted, and Settings has a
**How far back** panel that will fetch the rest, behind a dialog that says
what it costs in requests and minutes.

Officials are recognised before anything is downloaded, which is what makes
that safe. ETF2L is now asked *before* the download queue is built, and each
log is placed against the scheduled times: across 56 officials every log
started between 10 and 166 minutes after its scheduled time and never before,
so a three-hour window finds all of them. Without it the two-year window would
quietly lose 13 old officials.

**Matches appear rated as they arrive.** A sync fetches newest first, so last
night's game shows up within seconds — and now with its rating already on it,
scored against the stored pool, rather than blank until the whole history is
re-rated at the end.

**Sync reports from the corner**, beside demo downloads, instead of a bar
across the top of whichever page started it. It survives every trip to a match
and back. Rebuilding from stored data reports there too; Settings used to show
nothing at all while it ran.

**A dead logs.tf no longer takes the sync with it.** logs.tf went down during
testing and the whole sync failed on an index step that is only a supplement.
Now a source that cannot be reached is noted and stepped over. More
importantly, a log that could not be *connected* about no longer burns one of
its three attempts — two syncs during an outage would have parked a whole
history behind a manual retry — and three unreachable logs in a row end the
pass rather than spending an hour asking a server that is not there.

## An empty database says so

A wipe used to look exactly like a new install: no config, no matches, a setup
screen asking for a SteamID, and a fresh download of twelve years of logs with
a 200 MB backup sitting in the folder next door. Now the app checks before it
asks anything else, and offers to put the backup back.

The swap happens at the next start with nothing connected, because SQLite
holds the file open while the app runs. The old file is kept as `.replaced`
rather than deleted, and the restore is refused outright if the database has
anything in it.

Settings also shows the database path with a **Show in Explorer** button, and
the same for the backups folder.

## Movement and demos

- The player picker on the map is grouped by side — **Us · BLU** and
  **Them · RED** — with your own team first.
- Each life lists the points your team captured while it lasted: a life that
  took three points reads as the push it was, not just a long life.
- Combined logs: read the whole match, or one of its logs at a time, and the
  whole page follows the choice.

## Smaller things

- The match list filters by **class** and by **map**, and every filter now has
  a fixed place instead of shuffling when one of them is missing.
- Rows in that table alternate shades; eleven columns of numbers are hard to
  read straight across.
- The counts and the Sync button moved into the top bar, and Settings is a cog
  beside your name.
- Demo downloads keep running while you browse, with a progress card that
  takes you back to the match when it lands.

## One fix worth naming

Stored rating baselines did not come back as the numbers that went in.
`serde_json`'s default parser is not correctly rounded, and a real pool value
read back one bit high — enough to turn a player's tie with the pool into a
near miss and move a percentile by half a rank. The match page had been
reading baselines that way since it was written.
