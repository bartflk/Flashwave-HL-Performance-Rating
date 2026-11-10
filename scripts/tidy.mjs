// Clear away the extra worktrees, their build caches and the test copies of
// the database that piled up beside the project (Flashy, 6 Nov 2026: "make
// sure these double worktrees and builds get automatically cleaned up after
// releases and checked on" -- C: was down to 37 GB, 105 GB of it worktrees
// and scratch databases).
//
//   node scripts/tidy.mjs            clean what is safe to, then report
//   node scripts/tidy.mjs --report   only report (before every dev/build)
//
// Runs on its own at the end of `npm run release`, and as a report before
// every `npm run dev` and `npm run build`. What it removes, and only then:
//
//   - a worktree (other than the main checkout) whose branch is merged into
//     main, with nothing uncommitted, and no file touched for 2 days. Its
//     links into the main checkout (node_modules, dev-data) are unlinked
//     first, never followed. The branch is kept. Worktrees the Claude app
//     made itself (.claude/worktrees) are the app's: only their build cache
//     goes, by the same rule.
//   - a worktree's `target` (only a build cache) when nothing in it was
//     touched for 7 days, merged or not.
//   - in Claude's scratch folders for this project: copies of the database
//     (*.sqlite3 and their -wal/-shm), `target` folders and .exe files not
//     touched for 7 days. A scratch folder with a file named KEEP is left
//     whole.
//
// The main checkout's own `target` is sweep.mjs's (a 30 GB cap). Nothing
// here ever touches dev-data, the installed app's data, or a branch.

import { execFileSync } from "node:child_process";
import { existsSync, lstatSync, readdirSync, rmSync, rmdirSync, statfsSync, unlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const report = process.argv.includes("--report");
const GB = 1024 ** 3;
const DAY = 86_400_000;
const IDLE_WORKTREE = 2 * DAY;
const IDLE_CACHE = 7 * DAY;
/** Below this much free, the report says so loudly. */
const LOW_FREE = 50 * GB;
const fmt = (b) => (b < GB ? `${Math.round(b / 1024 ** 2)} MB` : `${(b / GB).toFixed(1)} GB`);
const days = (ms) => `${Math.floor(ms / DAY)} d`;

const git = (...args) => execFileSync("git", args, { cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] }).trim();

function lstat(p) {
  try {
    return lstatSync(p);
  } catch {
    return null;
  }
}

function list(p) {
  try {
    return readdirSync(p);
  } catch {
    return [];
  }
}

/** Bytes and the newest change under `p`, never following a link; `skip`
 *  names folders whose files do not count as being worked on. */
function walk(p, skip = new Set()) {
  const st = lstat(p);
  if (!st || st.isSymbolicLink()) return { bytes: 0, newest: 0 };
  if (!st.isDirectory()) return { bytes: st.size, newest: st.mtimeMs };
  let bytes = 0;
  let newest = 0;
  for (const name of list(p)) {
    const sub = walk(join(p, name), skip);
    bytes += sub.bytes;
    if (!skip.has(name)) newest = Math.max(newest, sub.newest);
  }
  return { bytes, newest };
}

/** Every link (a junction on Windows) under `p`, not looking inside them. */
function links(p, out = []) {
  const st = lstat(p);
  if (!st) return out;
  if (st.isSymbolicLink()) {
    out.push(p);
    return out;
  }
  if (st.isDirectory()) for (const name of list(p)) links(join(p, name), out);
  return out;
}

/** Delete `p`: its links first, as links, so a junction into the main
 *  checkout can never take dev-data or node_modules with it. */
function remove(p) {
  for (const link of links(p)) {
    try {
      unlinkSync(link);
    } catch {
      rmdirSync(link); // a directory junction; removes the link alone
    }
  }
  if (links(p).length > 0) throw new Error(`a link under ${p} would not unlink`);
  rmSync(p, { recursive: true, force: true, maxRetries: 0 });
}

const now = Date.now();
const lines = [];
let freed = 0;

// ---- Worktrees ---------------------------------------------------------------

const worktrees = [];
for (const block of git("worktree", "list", "--porcelain").split(/\r?\n\r?\n/)) {
  const path = block.match(/^worktree (.+)$/m)?.[1];
  if (!path) continue;
  worktrees.push({ path: resolve(path), branch: block.match(/^branch refs\/heads\/(.+)$/m)?.[1] ?? null, head: block.match(/^HEAD (\w+)$/m)?.[1] });
}
// The main checkout is git's first worktree, whichever copy this script
// was started from. Run from a side worktree, `root` is that worktree and
// the main checkout would otherwise look like one more merged worktree
// (audit, 10 Nov 2026): so only report there, and never touch the main one.
const mainPath = worktrees[0]?.path;
if (!mainPath) throw new Error("git listed no worktrees");
const fromMain = resolve(root) === mainPath;
if (!report && !fromMain) {
  console.log(`tidy: run from ${resolve(root)}, not the main checkout (${mainPath}); reporting only.`);
}
const destructive = !report && fromMain;
const within = (child, parent) => child === parent || child.startsWith(parent + sep);
/** Kept whatever git says: a real dev-data folder or any database copy. */
function holdsData(p) {
  const dd = lstat(join(p, "dev-data"));
  if (dd && !dd.isSymbolicLink()) return true;
  let found = false;
  const look = (q, depth) => {
    if (found || depth > 3) return;
    for (const name of list(q)) {
      if (name === "target" || name === "node_modules" || name === ".git") continue;
      const sub = join(q, name);
      const st = lstat(sub);
      if (!st || st.isSymbolicLink()) continue;
      if (/\.sqlite3$/i.test(name)) found = true;
      else if (st.isDirectory()) look(sub, depth + 1);
    }
  };
  look(p, 0);
  return found;
}
const appMade = (p) => p.startsWith(join(mainPath, ".claude", "worktrees") + sep);

for (const w of worktrees) {
  if (within(mainPath, w.path) || within(resolve(root), w.path)) continue;
  const name = w.branch ?? w.path;
  if (!existsSync(w.path)) {
    lines.push(`worktree ${name}: folder gone, pruned`);
    continue;
  }
  const work = walk(w.path, new Set(["target", "node_modules", ".git"]));
  const cache = walk(join(w.path, "target"));
  const idle = now - work.newest;
  let merged = false;
  let clean = false;
  try {
    execFileSync("git", ["merge-base", "--is-ancestor", w.head, "main"], { cwd: root, stdio: "ignore" });
    merged = true;
  } catch {
    // not in main yet
  }
  try {
    clean = execFileSync("git", ["status", "--porcelain"], { cwd: w.path, encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] }).trim() === "";
  } catch {
    // a worktree git cannot read: leave it
  }
  const state = `${fmt(work.bytes)}, cache ${fmt(cache.bytes)}, ${merged ? "merged" : "not merged"}, ${clean ? "clean" : "uncommitted changes"}, idle ${days(idle)}`;
  if (!destructive) {
    lines.push(`worktree ${name}: ${state}`);
    continue;
  }
  try {
    if (merged && clean && idle >= IDLE_WORKTREE && !appMade(w.path) && !holdsData(w.path)) {
      remove(w.path);
      freed += work.bytes;
      lines.push(`worktree ${name}: removed (${state}); branch kept`);
    } else if (cache.bytes > 0 && (idle >= IDLE_CACHE || (merged && clean && idle >= IDLE_WORKTREE))) {
      remove(join(w.path, "target"));
      freed += cache.bytes;
      lines.push(`worktree ${name}: build cache removed (${state})`);
    } else {
      lines.push(`worktree ${name}: kept (${state})`);
    }
  } catch (e) {
    lines.push(`worktree ${name}: kept, could not remove (${e.code ?? e.message})`);
  }
}
if (destructive) git("worktree", "prune");

// ---- Claude's scratch folders --------------------------------------------------

const slug = mainPath.replace(/[^A-Za-z0-9]/g, "-");
const scratchRoot = join(tmpdir(), "claude", slug);
const junk = (name) => /\.sqlite3(-wal|-shm|\.gz)?$/i.test(name) || /\.exe$/i.test(name) || name === "target";
let scratchBytes = 0;
let scratchJunk = 0;
for (const session of list(scratchRoot)) {
  const dir = join(scratchRoot, session, "scratchpad");
  if (!existsSync(dir)) continue;
  scratchBytes += walk(join(scratchRoot, session)).bytes;
  if (existsSync(join(dir, "KEEP"))) continue;
  const found = [];
  const look = (p) => {
    for (const name of list(p)) {
      const sub = join(p, name);
      const st = lstat(sub);
      if (!st || st.isSymbolicLink()) continue;
      if (junk(name)) found.push(sub);
      else if (st.isDirectory()) look(sub);
    }
  };
  look(dir);
  for (const p of found) {
    const { bytes, newest } = walk(p);
    if (now - newest < IDLE_CACHE) continue;
    if (!destructive) {
      scratchJunk += bytes;
      continue;
    }
    try {
      remove(p);
      scratchJunk += bytes;
      freed += bytes;
    } catch {
      // held by a running session: next time
    }
  }
}
if (!destructive) {
  if (scratchJunk > 0) lines.push(`scratch: ${fmt(scratchBytes)} in Claude's folders, ${fmt(scratchJunk)} of it old database copies and builds (npm run tidy)`);
} else {
  lines.push(`scratch: ${fmt(scratchJunk)} of old database copies and builds removed from Claude's folders (${fmt(scratchBytes - scratchJunk)} left)`);
}

// ---- The drive ---------------------------------------------------------------------

let free = null;
try {
  const fs = statfsSync(mainPath);
  free = fs.bavail * fs.bsize;
} catch {
  // not on this platform
}
if (destructive) lines.push(`freed ${fmt(freed)}`);
if (free !== null) lines.push(`${mainPath.slice(0, 2)} free: ${fmt(free)}${free < LOW_FREE ? "  -- LOW: run npm run tidy, or delete target (a build cache)" : ""}`);
for (const line of lines) console.log(line);
