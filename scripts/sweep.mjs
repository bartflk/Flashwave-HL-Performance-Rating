// Keep `target` from ballooning (Flashy: "so it doesn't balloon into 60 gigs
// again" -- hl-queue's had reached 68 GB, the main checkout's 25).
//
// Runs before every `npm run dev` and `npm run build` (so `npm run release`
// too). Cargo never deletes old build output on its own: every dev, test and
// release build adds its own copy, and the incremental caches keep a new
// session per build. When `target` is over the cap, this frees it in two
// steps, cheapest first:
//
//   1. the incremental caches (only make the next small rebuild a little
//      slower);
//   2. if still over, everything but the installers in release/bundle
//      (the next build is a full one: a few minutes).
//
//   node scripts/sweep.mjs            check, sweep if over the cap
//   node scripts/sweep.mjs --report   only say how big it is
//
// The cap is 30 GB, or HL_TARGET_CAP_GB. A file another program holds
// (a running dev app) is skipped, never waited on.

import { readdirSync, lstatSync, rmSync, existsSync } from "node:fs";
import { join, dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const target = process.env.CARGO_TARGET_DIR ? resolve(process.env.CARGO_TARGET_DIR) : join(root, "target");
const capGb = Number(process.env.HL_TARGET_CAP_GB) || 30;
const GB = 1024 ** 3;

/** Bytes under `p`, not following links. */
function size(p) {
  let st;
  try {
    st = lstatSync(p);
  } catch {
    return 0;
  }
  if (st.isSymbolicLink()) return 0;
  if (!st.isDirectory()) return st.size;
  let n = 0;
  let names = [];
  try {
    names = readdirSync(p);
  } catch {
    return 0;
  }
  for (const name of names) n += size(join(p, name));
  return n;
}

/** Delete `p`, saying so; a locked file is left for next time. */
function remove(p) {
  try {
    rmSync(p, { recursive: true, force: true, maxRetries: 0 });
  } catch (e) {
    console.log(`  kept ${p} (${e.code ?? e.message})`);
  }
}

const fmt = (b) => (b < GB ? `${Math.round(b / 1024 ** 2)} MB` : `${(b / GB).toFixed(1)} GB`);

if (!existsSync(target)) process.exit(0);
let total = size(target);
console.log(`target: ${fmt(total)} (cap ${capGb} GB)`);
if (process.argv.includes("--report") || total <= capGb * GB) process.exit(0);

// 1. The incremental caches, in every profile (debug, release, test builds).
for (const profile of readdirSync(target)) {
  const inc = join(target, profile, "incremental");
  if (existsSync(inc)) remove(inc);
}
total = size(target);
console.log(`  without the incremental caches: ${fmt(total)}`);
if (total <= capGb * GB) process.exit(0);

// 2. Everything else, keeping the installers a release just made.
for (const profile of readdirSync(target)) {
  const dir = join(target, profile);
  if (!lstatSync(dir).isDirectory()) {
    remove(dir);
    continue;
  }
  for (const name of readdirSync(dir)) {
    if (profile === "release" && name === "bundle") continue;
    remove(join(dir, name));
  }
}
console.log(`  swept: ${fmt(size(target))}; the next build is a full one`);
