# Flashwave.tf 0.13.0 beta — Beta 3

**On 0.5.0 or later (Windows)?** The app offers this update itself: take it
from the card, or Settings › Updates › Check for updates.

**New install on Windows:** download the `-setup.exe` and run it. **Windows
will say "Windows protected your PC"** — the installer is not code-signed.
Right-click the download → **Properties** → tick **Unblock** → **OK**. Check
it is my build first:

```powershell
Get-FileHash .\Flashwave.tf_0.13.0_x64-setup.exe -Algorithm SHA256
```

**Linux:** the `.AppImage` runs on most distributions
(`chmod +x Flashwave.tf_*.AppImage && ./Flashwave.tf_*.AppImage`); the `.deb`
is for Debian, Ubuntu and Mint (`sudo apt install ./Flashwave.tf_*.deb`).
They are built on GitHub a few minutes after this release appears.

The hashes are at the bottom. Updating keeps everything.

---

Round openings as PGL draws them, per class and per side. ETF2L back to
Season 12, with each season's banner and news posts. A first RGL tab. DPM and
accuracy measured against five years of logs.tf. Backups that stay small.
**Your demos are read again**: demo reading is version 14 (it was 12), so the
first start after updating works through every demo on this machine. It keeps
the computer busy for a while, so start it when you are not playing. Ratings
do not change; the model is still v13.

## Round openings

- **Where a class goes when the round starts**: a new view under Lives on a
  map, in the spirit of PGL's heatmaps. It shows the first 40 seconds of each
  round, or up to the round's first kill if that comes sooner, as hexes in
  the side's colour, brighter where more time was spent. A ring marks each
  side's main position: the spot the most rounds went through, counted from
  8 seconds in so the way out of spawn does not take it.
- **Compare with** anyone a demo on this machine followed on that map, each
  with their own class picker, drawn in gold and teal. **Their opposite
  Sniper** (or any class) picks whoever played it against you, match by
  match.
- **On the match page** too, under Kill by kill: each team's players of one
  class, following the round picker.
- **Spawn rooms are left out**, using each map's callouts. Steel now has its
  spawn rooms drawn, and Upward and Swiftwater got their missing RED spawns.
- **Every player's first life of a round counts**, including players who
  spawned in the freeze before the round started. A Read again button on the
  round openings catches one match up straight away.

## Missing demo parts

- **A server splits its SourceTV into parts** of about 18 minutes, and a log
  was linked to one of them, so a 9-round koth could show 3 rounds. Fetching
  a match's STV now looks on demos.tf for the other parts, asking up to nine
  of its players, and downloads the ones missing.
- **Your own recording fills the rest**: in rounds no STV part covers, your
  POV demo gives your own routes.
- The match page says which rounds the demos here cover, offers **Find and
  download the other parts**, and asks once when you open such a match.
  **Settings › Missing demo parts › Check for missing demos** lists every
  match with rounds missing. When demos.tf has no part for some rounds, it
  says so.

## Older ETF2L seasons

- **ETF2L Highlander back to Season 12**, for Clark. Those seasons' logs were
  never linked to their matches, so they were found in the logs.tf archive by
  map, time and lineups, then downloaded from drops.tf. The league data now
  holds 9,549 rated officials (it was 6,157). Seasons 12 to 18, 22 and 23
  have nearly every match. Seasons 19 to 21 are thinner: their ETF2L pages
  list few players.
- **Medals** fill a place ETF2L's standings left out from the season's
  results.
- **Tournaments**, a new page beside Seasons: every one-night cup,
  Experimental Cup, Nations' Cup and preseason cup, with each bracket's
  winner. Open one for its brackets round by round. Players' pages list
  their tournaments under their seasons: team, bracket, record and how far
  they went. Preseason cups no longer count in a season's record.
- **Cup brackets are not divisions**: on the Matches page, a cup's brackets
  (the Halloween cup's Werewolves and Zombies) are marked CUP and listed
  apart from Premiership to Open.
- **Every season's banner**, and **News of this season** on its page: its
  announcement, tiers, playoffs, Grand Final and awards posts, opened in the
  app's news reader.

## RGL

- **A first RGL tab, in development** (it says so the first time you open
  it), under RGL's logo and laid out like the ETF2L tab, for NA Highlander:
  - every season with its map pool and its divisions, best division first,
    with each division's ETF2L equivalent (Invite = Premiership … Newcomer =
    Open);
  - a team's results match by match, its roster with who left when, and its
    other seasons;
  - a player by SteamID or a Steam or rgl.gg link: bans, probation, and every
    season with team, division and W-L;
  - your own RGL card on Home;
  - each season's **standings**, worked out from its results, and the
    results week by week. Reading a season's matches takes about 40 seconds
    the first time; finished matches are then kept, so it opens at once;
  - the league's **bans**;
  - **Matches**: the live season's week, what's still to come and the
    results, with a division filter and countdowns;
  - every match opens its own page: both teams, each map's score, the date;
  - Home shows the next matches and the latest results, a team's page its
    record on each map, and a season's top three their medals;
  - **search by name**: players and teams, from the header or Players;
  - the Seasons page as cards, each with its Invite top three;
  - **bookmarks** for RGL teams and seasons.
- **Easy on RGL's servers**: the app keeps under RGL's published rate limit
  and waits when RGL says to. It asks for the same thing only once, even
  when two pages want it at the same time. A finished season, its teams and
  its matches are saved on this computer after the first read and never
  asked for again. For matches still to be played, one request to RGL's
  list of changes says which ones to read again.

## Stats

- **DPM and accuracy are measured against the whole logs.tf archive**: 209,658
  Highlander logs from 2021 to 2026, 3.76 million class games. Before, it was
  the ~125,000 this app had read. There are per-map scales for 291 class and
  map pairs.
- **Hover a coloured DPM or accuracy** to see where it stands on the class's
  scale, the top 30% / 10% / 1% marks, and how many games and players the
  scale comes from.
- **Stat colours**: red to green is the default; **osu!** now uses osu!'s
  current difficulty colours.
- **Sentry busting**: sentries and buildings destroyed per 30 minutes on the
  profile's classes, ranked against everyone on the class, plus SG and BLD
  columns on the scoreboard and the season tables.
- **In depth** counts officials by default, with an Officials / All games
  switch. Against each division now uses your opponent's division (it used
  your own), and each map counts once. A progress bar, Refresh, and *What
  these numbers are made of* show how many maps made it and why the rest did
  not.

## Your matches

- **Tournament dividers**: your officials grouped by tournament, each with a
  band naming it and its division, starting a week early so the scrims for it
  fall under it. Turn them off in Settings › Interface size.
- **Pug servers' games are pugs**: a protfx, tf2pickup or hlpugs log no
  longer shows as a scrim when you pug with the same friends. A kind you set
  by hand still wins.
- **Filters are remembered**: the format, class, map, sort and With STV stay
  as you left them across tabs and restarts. **Reset filters** sets them
  back.
- **A season's dates on a divider** at the top of the list, and custom dates
  too, rather than behind the Refresh and Import buttons.
- **Switched-on chips look switched on**: With STV and the other toggles are
  lit when they are on.
- **The kill map's heatmap is a gradient**, not scattered spots.

## Backups and storage

- **Backups stay small.** A copy leaves out the league officials' logs, which
  download again by themselves, and is compacted: 1.52 GB became 0.76 GB.
  The app keeps the newest copy and one a day before it, five at most and
  3 GB in total. Before, it kept the last five whatever their size.
- **Settings › Backups** shows what the app takes on disk, every copy with
  Restore and Delete, and what the database holds.
- **A Storage card** appears when there are more than two backups or the app's
  folder is over 5 GB.

## A calmer app

- **Plain errors**: a site that is down says so in one line ("trends.tf isn't
  answering right now; try again later."). The details are on hover.
- **Fewer requests**: a profile asks ETF2L once, a site that failed is left
  alone for a quarter of an hour, and pages that rarely change are kept
  longer.
- **Controls line up**: on Lives on a map and the kill map, every control is
  the same height and the labels sit on one line, with room between the
  rows of every control panel.
- **Narrow windows**: down to 900 px, the top bar, the match list, the
  scoreboards and every header fit. A header that runs out of room goes to
  two lines as a whole (the title above, its menu and search together under
  it), and no row leaves one control alone on a line. The map's zoom buttons have
  their icons back. Settings sections have room to breathe.
- **French, Spanish, Portuguese and Russian are complete.**

## Thanks

Clark for asking for the older seasons, and Icewind for drops.tf and demos.tf.
The DPM and accuracy scales and the older seasons' logs come from logs.tf's
archive, and the RGL tab reads RGL's public API. Map screenshots, medal art
and the kill map's icons are Valve's, from the Official TF2 Wiki.

**SHA-256**

```
aac0f47aac47fad44b808edb0ec20b24d799dec9c74cbb9b5b7200ed03d615d3  Flashwave.tf_0.13.0_x64-setup.exe
161acbbb139af67cfb89baf837d196f0a4efa4f30bf847f1dc9631243c9fb43b  Flashwave.tf_0.13.0_x64_en-US.msi
```

The Linux files are built on GitHub; their hashes are on the release page
beside each file.
