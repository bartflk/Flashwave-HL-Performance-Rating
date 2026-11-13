# tf2esports.com: features to bring into the RGL tab (and the app)

Looked at on 12 Nov 2026 (Flashy: "i want a lot of the site features copied as well, make a list").
tf2esports.com covers every league and format; the app's RGL tab covers RGL's NA Highlander, from
RGL's public API (api.rgl.gg), logs.tf (via drops.tf by Icewind) and the app's own ratings. Each
item says where its data would come from, and whether it can be built without asking a service
more than it should (docs/database-rules.md, the RGL rules in `hl_ingest::rgl`).

Legend: **Now** buildable from data the app reads already · **Logs** needs each match's logs
(found on logs.tf, read from drops.tf, kept on disk) · **Heavy** needs many requests or a new
source; only with care · **No** not possible from public data.

## Site-wide

| Feature on tf2esports | What it does | For the app | Data |
|---|---|---|---|
| Live ticker | A strip across the top: every match being played now, score, week, BO, LIVE dot | RGL tab: tonight's and live matches in a strip under the head | Now |
| "Show results" toggle | Hides every score (spoiler mode), one switch in the header | A spoiler switch for the RGL and ETF2L tabs | Now |
| Search with ⌘K | One box for teams, players, matches | RGL search exists; a keyboard shortcut and recent searches | Now |
| Language, theme switch | | The app has both | — |
| Newcomers page | Formats, leagues, divisions, jargon buster, FAQ, where to watch | A short "New to RGL Highlander?" card: divisions, season shape, how matches work | Now (written by hand) |
| Discord bot, RSS feed, public API, embeds | | Out of scope for a desktop app | No |
| Forums, comments, prediction polls | Needs accounts and a server | Out of scope | No |

## Matches

| Feature | What it does | For the app | Data |
|---|---|---|---|
| Upcoming / Results tabs, Live now at the top | | RGL Matches page has the week and days; add Live now and a Results tab | Now |
| Filter sidebar | Calendar, status, region, division, format, teams | Division, week, team, status (to play / played / forfeit) filters | Now |
| Add to calendar | iCal of the matches shown, or one team's | Export an .ics of a team's matches; RGL also has `/profile/{id}/calendar` per player | Now |
| Form marks beside a team | Last five results as small W/L dashes | Beside each team in match rows | Now |
| BO and league chips on a row | "BO2 · Week 5 · Division 2 · 6v6" | Week, division (with its ETF2L tier) and map chips | Now |
| Streams and VODs, Theatre | Twitch streams by casters and POVs, viewer counts | Not public data the app has | No |

## A match

| Feature | What it does | For the app | Data |
|---|---|---|---|
| Score header with both teams' rank and points | "#7 NA · 1633 pts +37" | Each team's place in the season table and its form | Now |
| Per-map stats from the logs | Team comparison: score, K, D, damage, heals, ubers, drops | From the match's logs (found already) | Logs |
| Match awards | MVP, Medic MVP, top fragger, most headshots | From the logs and the app's rating | Logs |
| Player table | Class, K, A, D, DMG, DPM and its difference to the class average, DT/M | Every player of the match, the app's rating beside them, as ETF2L's lineups | Logs |
| Medic matchup | The two medics side by side: heals, ubers, drops | | Logs |
| Player matchup | Pick a player each side, compare their numbers | Class by class, the two players facing each other | Logs |
| Map stats and recent matches of both teams | | Each team's record on the match's maps, last meetings (head-to-head exists) | Now |
| Kill feed, round timeline, 3D replay | | The app has these for stored logs: the match page links into them | Logs |

## Rankings

| Feature | What it does | For the app | Data |
|---|---|---|---|
| Team rankings with points and +/- | Elo-like points per format and region, last update, rank change | An RGL HL power ranking from the seasons the app has read (the current and the last), rank change week to week | Heavy (a season is ~150 requests the first time, then on disk) |
| Peak rank, ranking history chart | | On a team's page, once rankings exist | Heavy |
| Roster under each team in the table | | | Now |

## A team

| Feature | What it does | For the app | Data |
|---|---|---|---|
| Overview / Players / Matches / Events / Achievements tabs | | Tabs on the RGL team page | Now |
| Also known as | Former names | From its linked seasons' names | Now |
| Headline cards | World/region rank, last 12 months W–L and win rate, titles, podiums, active since | Record across linked seasons, titles (final rank 1), podiums, first season | Now |
| Recent form strip | W/L squares, newest first, then the results | | Now |
| Trophy cabinet | Every podium: place, season, division, year | From linked seasons' final ranks | Now |
| Map pool | Record per map, pick/ban counts, mid-win % | Record per map exists; add win %, rounds, across seasons | Now (rounds), Logs (mids) |
| Most-played opponents | Record against each, forfeits excluded | Across the linked seasons' matches | Now (a season's matches are read once) |
| Top performers | Per-player K/D and DPM, per season | From the match logs | Logs |
| Roster with join dates and transfers | | Roster exists; add a transfers timeline across seasons | Now |
| Follow, Compare | Follow needs an account; Compare puts two teams side by side | Compare two RGL teams | Now |

## A player

| Feature | What it does | For the app | Data |
|---|---|---|---|
| Medal badges by the name | Champion / runner-up / third | From their seasons' final ranks | Now |
| Headline cards | Titles, matches, maps and win %, career span, events and teams | From their RGL seasons | Now |
| Recent form and results | | Their teams' matches while on the roster | Now |
| Class ranking | Rank among every player of the class, rating, key stats | The app's rating is exactly this: show it beside the RGL career | Now (app data) |
| Trophy cabinet, career timeline by year | Teams by year with division and format | | Now |
| Career highs | Best DPM, most kills in a map, with the match | From the logs the app has rated | Logs |

## Players board

| Feature | What it does | For the app | Data |
|---|---|---|---|
| One row per player and class, heat-mapped cells, trend sparkline | 30+ stat columns, filters by time, region, division, event, map, team, min maps; per 30 min or totals | The app's Rankings view does this for ETF2L; an RGL division filter once RGL logs are rated | Logs |
| Transfers | Every roster move | RGL rosters have join/leave dates per season | Now |

## Stats and events

| Feature | What it does | For the app | Data |
|---|---|---|---|
| Maps: most played, by mode; a map's usage, history, pick/ban, callouts | | Most-played RGL HL maps by season (season map pools), and the app's own map pages | Now |
| Weapons meta | | From logs | Logs |
| Compare teams | | See team Compare | Now |
| Events: ongoing and completed, teams count, dates, format, region | | RGL's seasons list is this; add dates (first and last match) and team counts | Now |

## Suggested order for the RGL tab

1. Matches: Live now, Results tab, filters, form marks, chips, spoiler switch, calendar export.
2. Team page: tabs, headline cards, form strip, trophy cabinet, opponents, aliases, Compare.
3. Player page: medals, headline cards, form, timeline, the app's rating beside the RGL career.
4. Match stats from the logs: team comparison, awards, player table with ratings, medic and player matchups.
5. Seasons as events: dates, team counts, podiums; most-played maps.
6. A power ranking, once the seasons it needs are on disk.
