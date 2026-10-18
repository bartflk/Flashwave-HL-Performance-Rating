# Flashwave.tf 0.11.0 beta — Beta 1

**On 0.5.0 or later (Windows)?** The app offers this update itself: take it
from the card, or Settings › Updates › Check for updates.

**New install on Windows:** download the `-setup.exe` and run it. **Windows
will say "Windows protected your PC"** — the installer is not code-signed.
Right-click the download → **Properties** → tick **Unblock** → **OK**. Check
it is my build first:

```powershell
Get-FileHash .\Flashwave.tf_0.11.0_x64-setup.exe -Algorithm SHA256
```

**Linux:** the `.AppImage` runs on most distributions
(`chmod +x Flashwave.tf_*.AppImage && ./Flashwave.tf_*.AppImage`); the `.deb`
is for Debian, Ubuntu and Mint (`sudo apt install ./Flashwave.tf_*.deb`).
They are built on GitHub a few minutes after this release appears.

The hashes are at the bottom. Updating keeps everything.

---

The first beta. Every Highlander player's logs.tf history since 2012 is now
built in, the ETF2L tab is laid out like etf2l.org, profiles are redone, every
table sorts and every number says what it means when you hover it, and themes
can be your own, with the map behind them. Ratings are unchanged (model v11);
20 of 770 MVPs change under fairer rules.

## logs.tf history, built in

- **93,574 Highlander players** with five or more logs since 2012, in a
  ~6 MB summary inside the app: the names they used, their career by class in
  competitive games and in pugs and lobbies, where they rank among each
  class's regulars, and a rating a year. Duplicate uploads and combined logs
  are left out.
- **The Players search finds anyone in it**, by any name they played under or
  their Steam ID, and every profile and your Career tab show a **logs.tf
  career card**.
- **Opponent strength**: a match page, the ETF2L match page and a team's
  page show each side's average career rating over the players with 20 or
  more competitive games, who was favoured and whether it was an upset.

## ETF2L

- **One ETF2L tab, laid out like etf2l.org**: Home with your next match,
  season banner, division leaders, results and news; Matches opening on a
  **Week calendar**; Teams; recruitment posts; and a search over players and
  teams.
- **Every official has a page**: score or countdown, maps, lineups with
  ratings, head-to-head, logs and demos. **Mercs** show in the lineups on the
  side the logs put them, tagged, with whether they joined a team later.
- **Group stages and a Challonge-style playoff bracket** for every division,
  with a gold grand final.
- Tables styled, with alternating rows and the theme's colours, and banner
  text on smoked glass. ETF2L's logo marks ETF2L things across the app.

## Profiles

- **Redone as tabs**: Overview, Rating, Fights & aim, Maps, Career and
  Teammates. The Overview leads with the headline numbers — time played,
  record, damage per minute, accuracy and K/D.
- **Also known as**: every name they used, each in its own pill.
- **Career laid out as etf2l.org's**: matches by competition, teams, team
  history and achievements. Medals and MVPs grouped, one explanation each.

## Every table sorts, every number explains itself

- **Click a column to sort**: 36 of 38 tables, remembered per table.
- **Hover a statistic** to see what it is, how it is counted and where it
  comes from — the scoreboard, the rating's parts, Fights, Aim and more.
- **A spinner wherever the app waits**, and a small one in the corner of
  anything being fetched again, so old numbers are never taken for new ones.

## Themes

- **Six more themes** (Badlands, Upward, Process, RED, BLU, Mann Co.) and a
  **Hatsune Miku** one, just for fun.
- **Make your own** from four colours, the app repainting as you pick, and
  share it as a code.
- **Map backdrops**: each map theme shows its map, faded behind a profile's,
  a team's and a match's header — or behind the whole page, on both sides of
  the content like a Steam profile. Settings › Theme › Map backdrop.
- A new logo, drawn in the theme's colours, and the line under the top bar
  follows the theme too.
- **Interface size**, 80–200%, for 2K and 4K screens; Ctrl +, Ctrl − and
  Ctrl 0 from anywhere.

## Maps and demos

- **Lives on a map is fast** with thousands of lives, drawn on a canvas, and
  has a **Most used** view, **last N matches**, **situation filters** (capped,
  after a cap, mid-fight) and a place on your own Profile (Emiel).
- **Mass import**: a player's demos on one map from demos.tf, picked, run
  and managed in one window (Emiel).
- **The kill map**: zoom and full screen, a callouts menu, a legend that
  toggles each layer, TF2's own icons for deaths, sentries, dispensers and
  teleporters, and top-down images for 38 more maps from demos.tf (Emiel).
- Clearer controls on both maps and on Kill by kill: labelled groups with an
  icon on every option, and each round marked with the side that won it.

## Matches

- **Tabs on a match's page** that stay on screen as you scroll: jump
  straight to the Scoreboard, Rounds, Kill by kill or any other section.
- **Ping is a scoreboard column**, sortable, with the lowest, highest and
  any spikes on hover, instead of a panel of its own.
- **The round timeline**: minute labels, a line under the pointer with the
  time across every lane, the uber and Medic markers centred, and the
  minute lines where they belong.
- **Sorting by a class** (Kills on Soldier and so on) takes 0.4 s instead of
  21.
- Importing **a part of a combined log** says so and opens the combined one.
- demos.tf's files go back to 14 October 2019, and the app says so.
- Logs deleted from logs.tf are given up on instead of retried every sync.

## MVPs

- A player the roster does not place — a merc, a ringer ETF2L never listed —
  can be MVP on the side the final's logs put them.
- The playoff run counts in proportion to its maps, its full 30% only from
  four maps on. 20 of 770 MVPs change, each to the better final.

## Thanks

Emiel for Lives on a map, the mass import and the demos.tf map overviews (his
pull request), and Icewind for demos.tf and drops.tf. Map screenshots, medal art and the kill map's icons are Valve's,
from the Official TF2 Wiki.

**SHA-256**

```
DEE6BA4D1861215776BA2EB472CAB7E800D5668F26E2CAF13BBBD9A000DC2D89  Flashwave.tf_0.11.0_x64-setup.exe
9EDE462E5FE4AB36F7EADA80D84F8CF1754BFA24B43D5E08B8ADA3D8025B19EB  Flashwave.tf_0.11.0_x64_en-US.msi
```

The Linux files are built on GitHub; their hashes are on the release page
beside each file.
