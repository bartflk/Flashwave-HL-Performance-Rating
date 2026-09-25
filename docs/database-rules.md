# Working on the database without breaking it

On 25 September 2026 this database was corrupted twice in one afternoon.
Nothing was lost — the app's own backups covered both — but it cost the day
twice, and neither failure was exotic. Both were the same mistake made two
ways, and both are now prevented rather than remembered.

## What happened

**The first time.** A scratch copy was made with `cp hl.sqlite3 elsewhere`
while the app had the database open. A SQLite database in WAL mode is *two*
files: the write-ahead log holds committed pages the main file does not have
yet. Copying one without the other gives a torn database, and it reads as
`database disk image is malformed`.

**The second time.** The CLI was run against the app's database while the app
was running or had just been running. The file ended up full-size with a
write-ahead log three hours out of step beside it, and every query returned
`malformed database schema`.

## The rules, and what enforces them

**1. One writer. The app holds the database while its window is open.**

`hl_ingest::lock` takes an exclusive handle on `app.lock` beside the database
for as long as the app runs. Every CLI command that opens the database calls
`lock::require_free` first and refuses while the app has it:

```
Error: the app has this database open (...).
Close the app window and run this again.
```

The lock is a held file handle, not a PID file, so the operating system
releases it however the process ends — a crash leaves nothing stale.

`--force` overrides it. Use that only when you know the window is shut.

**2. Never `cp` a live database. Ask SQLite for a copy.**

```bash
hl copy "path/to/working-copy.sqlite3"
hl --db "path/to/working-copy.sqlite3" validate sniper
```

`hl copy` uses `VACUUM INTO`, which writes a consistent single file while the
original is still in use. That is also how `hl_ingest::backup` has always
worked, which is why every automatic backup survived both incidents.

If you must copy by hand, copy **all three** files together —
`hl.sqlite3`, `hl.sqlite3-wal`, `hl.sqlite3-shm` — or none of them.

**3. Measure on a copy.**

Anything exploratory — a validation sweep, a weights experiment, a
one-off query — runs against a copy. The live database is the app's.

**4. The backups are the safety net, and they work.**

A copy is taken before every sync and every rebuild, five are kept, and they
live in `backups/` beside the database. Both recoveries came from there.
Settings shows them and can put one back; a database that will not open at
all is moved aside and offered the same choice (`hl_ingest::restore`).

Keep one copy somewhere other than `%APPDATA%`. The uninstaller's "delete
application data" box takes the backups folder with it.

## If it happens anyway

1. Stop. Do not run anything else against the file.
2. Move it aside — do not delete it. A file this app cannot read is still the
   only copy of what was in it.
3. Check the backups: `hl --db <backup> stats`, or open Settings › Backups.
4. Put the newest good one back, and re-derive. Ratings, round maps, fights
   and demo links all rebuild from the stored logs; only the logs themselves
   are irreplaceable, and they are the part that is backed up.
