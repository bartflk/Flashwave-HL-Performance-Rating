# Flashwave.tf 0.12.0 beta — Beta 2

**On 0.5.0 or later (Windows)?** The app offers this update itself: take it
from the card, or Settings › Updates › Check for updates.

**New install on Windows:** download the `-setup.exe` and run it. **Windows
will say "Windows protected your PC"** — the installer is not code-signed.
Right-click the download → **Properties** → tick **Unblock** → **OK**. Check
it is my build first:

```powershell
Get-FileHash .\Flashwave.tf_0.12.0_x64-setup.exe -Algorithm SHA256
```

**Linux:** the `.AppImage` runs on most distributions
(`chmod +x Flashwave.tf_*.AppImage && ./Flashwave.tf_*.AppImage`); the `.deb`
is for Debian, Ubuntu and Mint (`sudo apt install ./Flashwave.tf_*.deb`).
They are built on GitHub a few minutes after this release appears.

The hashes are at the bottom. Updating keeps everything.

---

Season awards for every division, season tables that rank everyone fairly
against who they played, ETF2L data that keeps itself up to date, combined
logs read map by map, and a scoreboard you set up yourself: any stat, colours
by class and map, ratings green and red like HLTV's. **Ratings change**: the
model is v13 (it was v11), and every match is rated again on the first start.

## Season awards

- **Every division's season ends with three awards**, judged on the whole
  season: the **Season MVP** (the champions' best player), up to five
  **EVPs** (the best seasons on any team) and the **best player of each
  class** (from the three medal teams). An EVP is not also the best of their
  class; the next medallist takes it.
- Only games for your own team count; merc games show in the tables but not
  in the awards. An award needs most of the season: 60% of the most officials
  anyone in the division played, never fewer than four.
- **Medals for subs**: a player who joined the team later that season and was
  on its roster at the end gets the medal. Season winners are checked against
  ETF2L's final standings.

## Season tables and ratings

- **Rating v13.** A season rating is adjusted for how strong the teams faced
  were, and pulled towards 1.00 by three average officials, a number measured
  on the league's games rather than picked. Round impact counts half and
  deaths half as much; payload deaths are weighed on their own.
- **Team strength comes from results**: the rounds a team won that season, and
  against whom, not how its players' logs rated.
- **A near tie goes to output**: two season ratings within 0.005 are decided by
  the median kills per 30 minutes and damage per minute.
- **Everyone is listed**: 3 officials to be ranked (was 4), the rest shown
  greyed out (function). Click a row for the working: every official, its
  opponent's strength and what it added.
- Tables count officials, not logs, and every season has a data check.

## League data that stays current

- **Every sync fetches the league's newest officials**, from drops.tf first,
  a few minutes at most each time, so the season tables, medals and MVPs keep
  up between releases. **Settings › League data** shows how current it is and
  catches up in one go.
- A daily ETF2L check, and team names on officials you did not play.

## Combined logs

- **A log of several maps finds its parts** on logs.tf, puts its rounds on the
  right maps and its parts on the right sides, checked against ETF2L.
- **Read it map by map**: the Reading picker groups the logs by map.
- The round timeline jumps into each part's demo, and **movement works** on
  combined logs: every STV linked to a match is read.
- demos.tf is asked about the match's own players, and a match page can ask
  it again.

## The scoreboard

- **Columns**: pick any stat — kills, assists and deaths per 30 minutes,
  heals, ubers, drops, headshots landed, damage per kill and more — in logs.tf's
  order, remembered.
- **Hover a class icon** for each class played: time, K / D / A, damage,
  accuracy, and airshots, backstabs or headshots on the class that makes them.
- **DPM and accuracy coloured by class**, osu!-style or red to green (Settings
  › Stat colours), and **by map** where the class has enough games there:
  300 DPM on steel is a great Sniper game, on product an ordinary one.
- **Rank by Class %**: sort DPM and accuracy by how good they were for the
  class, so a Medic's best game can top the table.
- **Ratings coloured like HLTV's**: green above 1.05, red below 0.95, grey in
  between.
- **Head to head**: two players compared log by log, each rating's working.
- Impact on the scoreboard, and the table runs to the section's edges.

## Your matches

- **Every stat you played**, a tick away in Columns and sortable: damage
  taken, heals, ubers, airshots, health packs, longest killstreak, per minute
  and per 30 minutes, and **accuracy**.
- The same **Class %** sort over your whole history, and DPM and accuracy on
  their class's colours.

## A cleaner app

- **One control height**: every button, dropdown, chip and text box is the
  same size, with the theme's colours, and anything clickable has a shadow.
- **Descriptions moved behind a ? mark** beside each heading, with a new mark
  that sits centred; the Kill by kill tabs have a "How it's counted" instead.
- Text on one type scale, the match page's sections on one padding, its header
  laid out in pills, and loading lines wherever the ETF2L tab waits on ETF2L.
- The profile's rank popover and the match-kind menu no longer hide under the
  tabs; the ETF2L refresh sits beside the search.
- **French, Spanish, Portuguese and Russian are complete.**

## Thanks

function for the ranking threshold, and Icewind for drops.tf and demos.tf.
Map screenshots, medal art and the kill map's icons are Valve's, from the
Official TF2 Wiki.

**SHA-256**

```
HASHES
```

The Linux files are built on GitHub; their hashes are on the release page
beside each file.
