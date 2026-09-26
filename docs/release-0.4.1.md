# Flashwave.tf 0.4.1

Windows only. Download the `-setup.exe` and run it.

**Windows will say "Windows protected your PC"** — the installer is not
code-signed. Right-click the download → **Properties** → tick **Unblock** →
**OK**, and it runs with no warning at all. Check it is my build first:

```powershell
Get-FileHash .\Flashwave.tf_0.4.1_x64-setup.exe -Algorithm SHA256
```

The hash is at the bottom. Updating keeps everything.

---

A stability release. Mostly things testers found.

## When something breaks, you can now send it

**The app no longer goes black.** There was no error boundary anywhere, so a
single fault in one panel killed the whole window — which is what you saw if
you picked a log from a combined upload. A crash is now contained to the
panel it happened in, with the rest of the page still working.

**Settings › Problems** lists everything that has gone wrong since the app
started, and **Copy report** puts it on the clipboard as markdown with the
version and your database counts already filled in. If you are reporting
something, paste that instead of a screenshot.

**Failed syncs say why.** "2 failed" has become "could not reach the server"
or "the server does not have this log" — the difference between worth
retrying and never will be.

## A finished demo starts a sync

Play a match, alt-tab, and it is already there. The app watches your TF2
folders and waits for a demo to finish being written, then syncs and tells
you what it is doing. logs.tf is not instant, so it tries a few times before
saying so.

## Fixed

- **Custom dates broke the page and a reload would not clear it** (zaag).
  The period is saved across restarts and was never checked when read back,
  so a bad one broke every render, F5 included. The dates also no longer
  overlap the controls beside them.
- **Picking a season shoved the filter row out of line.** The date range now
  sits in the bottom-right corner with its space always reserved, so
  choosing a season changes nothing else.
- **The Aim tab showed your aim under someone else's name** (zaag). Picking
  another player changed every tab but that one, so you were reading your
  own numbers as theirs. It now says whose aim it is — yours, on any class —
  and offers a button back. Aim for *other* players needs the demo pass to
  record who was shooting, which is coming.
- **Head-to-head bars were scaled for the old 0–100 rating**, so a real gap
  between two players drew a sliver and every matchup looked even.
- **Three maps had no overview image** because their names were read wrong:
  `koth_product_rcx`, `pl_badwater_pro_v12`, `tow_tetsudo_b10a`.

## Less clutter

- **"Combined from N logs" is gone.** Those logs are links, so they sit with
  logs.tf and demos.tf in the header.
- **The "Reading" dropdown** no longer has a panel to itself; it is in the
  scoreboard header, beside the numbers it changes.
- **Rounds show ubers and numbers.** Each round has two strips under its
  lanes: who was up players and who held the uber, across that round. A
  round lost while down a player reads very differently from one lost even.
- **Fewer explanations.** Panels that described themselves at length to
  someone already looking at them have stopped.

---

**SHA-256**

```
A0345B9540F249781E5F6A73F3A25D195285C982A9B2B40ED25DA4B42B254377  Flashwave.tf_0.4.1_x64-setup.exe
6A4069EE7CEE522EF8B916F13E780D133A3C3923DC19A22D8BAFE88125B79B2A  Flashwave.tf_0.4.1_x64_en-US.msi
```
