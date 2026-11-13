# Design principles: how Flashwave.tf looks and behaves

Written 12 Nov 2026 (Flashy: "make a file with stylistic choices and ux design principles"), from
the app as it is and from tf2esports.com, which does a lot of the same job well. Anything new,
on any tab, follows this. When it and the code disagree, the code is usually older: fix the code.

## 1. The look

**Palette.** TF2's press kit: warm browns, cream text, the game's orange as the one accent
(`ui/src/index.css`, `:root`). Colours are roles (`--panel`, `--muted`, `--accent-text`, `--rule`),
never literals in a stylesheet, so every theme works. tf2esports uses a cool near-black (#111318)
with one orange (#e75427): the lesson is the same, **one accent, used sparingly**: for what can be
clicked, what is live, what is yours. Not for decoration.

**Fixed meanings.** RED and BLU, kill and death, won (green) and lost (red) are never themed and
never reused for anything else. A winner's score is bright, a loser's is muted, never coloured red.
The class icons are the game's. A division's colour is its tier on the ETF2L ladder, so an RGL
"Advanced" and an ETF2L "High" look alike on purpose.

**Type.** The display face (uppercase, letter-spaced) for panel titles, eyebrows and big numbers;
the text face for everything read. Numbers are tabular (`font-variant-numeric: tabular-nums`) in
every table and score. Sizes are the scale's (`--fs-*`), never a pixel value.

**Surfaces.** A page is panels on the background: `panel e2-panel`, a head row (`e2-panel-head`:
title left, a quiet count or link right), then content. Radius from `--radius`. tf2esports uses
12px cards with a 1px border and no shadow: depth comes from the surface colour, not shadows.
Hover is a faint accent tint (`rgba(var(--accent-rgb), 0.1)`), not a colour change of the text.

**Density.** Lists are dense and aligned, as tf2esports' match rows and tables are: one line per
item where it fits, columns that line up down the page, secondary facts as small chips
("Week 5 · Division 2 · BO2") rather than sentences. Heat-mapped numbers (green to red against
the pool) where comparing many players matters, as on the Players board.

## 2. Layout

- **Main column and a rail.** A page's main content on the left; context on the right (form, head-to-head,
  logs, "you on RGL"). The rail drops under the main column on a narrow window (`e2-match-grid`).
- **Aligned rows, equal heights, consistent padding.** Two panels side by side are the same height;
  a list's columns line up from the first row to the last; nothing sits a few pixels off.
- **Wrap as a whole.** A header that does not fit puts the title on its own line and the controls
  together under it (`useOneLine`); never one control dropping alone onto a line.
- **Odd one out.** In a grid of two, a last panel alone takes the row; a long list in it splits into
  two columns, a short one does not.
- **The trail.** Every page below a section has breadcrumbs, and Back (mouse button 4, Alt+←) steps
  back within the tab, through the same pages the trail shows.

## 3. Behaviour

- **Every state is designed.** Loading says what is being read, and how long it takes when that is
  long ("about 40 seconds the first time; you can look elsewhere meanwhile"). Empty says why
  ("Their first meeting on RGL", "A forfeit has no logs"), never a blank panel. An error is an
  `ApiError` with the cause and, where one helps, a way to try again.
- **One panel failing is one panel.** A page is built from parts that fail on their own: a team RGL
  will not give costs its lineup, not the match page.
- **Explain in a sentence.** A hint under anything not obvious, in plain words ("RGL lists rosters, not
  who played"). Numbers carry a title saying what they are ("Maps won and lost").
- **Time.** Upcoming things count down; recent ones say "2 h ago"; older ones show a date, with the
  year only when it is not this year. Week names stay the league's ("Week 3 - Product").
- **Spoilers.** Scores are the thing a person may not want to see yet: a switch hides them app-wide
  (tf2esports' "Show results").
- **Yours first.** Your team, your matches and your rating are marked wherever they appear (`mine`),
  and "You on RGL" / "Your teams" lead their pages.
- **Everything clickable looks it, and opens the thing.** A team name opens the team, a score the match,
  a log the log in the app. Rows are keyboard-reachable (`tabIndex`, Enter opens: `openOnKey`).
- **Search** waits until typing stops and needs a few letters before it asks a service.
- **Every string goes through `t()`** and ships in all five languages in the same commit.

## 4. Being a good guest

The data is other people's free services: RGL's API, logs.tf, drops.tf (by Icewind), trends.tf,
ETF2L. Ask once, keep what cannot change on disk, never per keystroke or per row, never in a loop
without a cache. A page that needs many requests says so and reads them at the service's pace.
Credit the source where its data is shown (drops.tf by Icewind in particular).

## 5. Checking it

Every new screen is looked at in the browser preview (`npm run ui` mock data, a worktree's own
port) at a desktop and a narrow width before it is called done: alignment, equal heights, padding,
every control styled, loading/empty/error states, the trail and Back.
