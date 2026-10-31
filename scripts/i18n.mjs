// Keep the translation files in step with the code (PLAN Q13).
//
//   npm run i18n            report: what each language is missing, and what
//                           it still translates that the app no longer says
//   npm run i18n -- --write bring every file up to date: add missing keys
//                           with an empty value, drop stale ones, sort, and
//                           refresh lang/template.lang for new languages
//                           and lang/en.lang for corrections to the English
//
// The English text is the key (see ui/src/lib/i18n.ts), so this finds every
// `t("...")` in the UI and compares. An empty value means "not translated
// yet" and shows the English, which is why adding a key never breaks a
// screen and a volunteer can translate as much or as little as they like.
//
// Only double-quoted literals are read. `t(someVariable)` cannot be found by
// reading the source, so a label passed through a variable -- the nav tabs
// are the example -- has to appear as a literal somewhere this script can
// see; see LITERALS below.

import { readFileSync, writeFileSync, readdirSync, statSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const src = join(root, "ui", "src");
const dir = join(root, "lang");
const write = process.argv.includes("--write");

// Labels that reach t() through a variable rather than a literal.
const LITERALS = [
  // App.tsx nav
  "Matches", "Profile", "Teammates", "Teams", "Players", "RGL",
  // RglPage.tsx: RGL divisions on the ETF2L ladder (rgl.rs)
  "Premiership", "High", "Mid", "Low", "Open",
  // StvGate.tsx: what each STV-only panel would show
  "Whether a Spy was cloaked is only in a demo: every hit on an invisible Spy, who found them, and when.",
  "Where the cart was is only in a demo: every second BLU was up players and the cart stood still, and how far it moved after each won fight.",
  "Where all eighteen players stood, second by second, is only in the STV demo: who anchors, who rotates, which callouts each player lives in.",
  // AnalysisPanel.tsx tabs
  "Kill map", "Play-by-play", "Fights", "Damage and kills by class", "Aim", "Timeline",
  // ProfilePage.tsx kind filter, and ContextBadge's KIND_PLURAL
  "All games", "Officials", "Scrims", "Pugs",
  // From the Rust side: the fights card (seasons.rs) and the profile headline.
  "% of fights",
  "% of fights you opened or died opening",
  "% of rounds",
  "% of your deaths",
  "% of your kills",
  "Clean-ups",
  "Deaths during your uber",
  "Deaths to flankers",
  "Deaths to their Sniper",
  "Deaths traded",
  "Died near a spot you had already got two kills from that life. Shown for reference: it does not predict losing.",
  "Died right after your kill",
  "Drops",
  "Dying while your team's uber is in use.",
  "Enemy ubers popped right after your damage on the Medic.",
  "Fight KAST",
  "Fights you were alive for where you got a kill or assist, survived, or had your death traded: HLTV's KAST, per fight.",
  "First death of the round",
  "First kills of fights you got.",
  "First pick of the round",
  "Forced ubers",
  "Killed by a Scout, Spy or Soldier: the threats from the side and behind.",
  "Killed by the enemy Sniper.",
  "Kills traded back",
  "Kills while your team was already up a player. Not bad, but they rarely decide a fight.",
  "Medic, Demoman, Heavy or Pyro killed while their team held a ready uber.",
  "Medics killed holding a ready uber.",
  "Opening duels won",
  "Opening kills",
  "Picks into a ready charge",
  "Rounds where the first kill was yours.",
  "Rounds where you were the first to die.",
  "Stayed put and died",
  "The first kill of a fight (more than 10 s after the last one): how often it was yours rather than you dying.",
  "Trades",
  "You died within 3 s of your own kill: often a sign of not repositioning.",
  "You killed back within 3 s of losing a teammate.",
  "Your team killed back within 3 s, so the death opened something (HLTV's traded-death rule).",
  "Your team lost someone within 3 s of your kill, so it opened nothing.",
  "deaths",
  "drops",
  "forces",
  "kills",
  "per 10 min",
  "rounds",
  "Sniper duel, career",
  "Medic picks, career",
  "Enemy Medics you killed, across every rated game on this class.",
  // From the Rust side: rating component names and units (model.rs) and
  // the sync stage names (sync_commands.rs). Regenerate if those change.
  "% of fights",
  "% of kills",
  "Backstabs",
  "Caps",
  "Caps into a defence",
  "Caps with your team dead",
  "Spawns delayed by your caps",
  "Damage / min",
  "Deaths",
  "Deaths to flankers",
  "Drops",
  "Fight KAST",
  "Fight KAST, engaged",
  "Fight swing",
  "Fight swing, shared",
  "Headshot share",
  "Healing",
  "Impact assists",
  "Impact kills",
  "Kills in context",
  "Kills not traded",
  "Matching demos.tf",
  "Medic picks",
  "Opening duels",
  "Reading ETF2L",
  "Refreshing your profile",
  "Resolving each round's map",
  "Scanning your demos folder",
  "Sniper duel",
  "Stationary deaths",
  "Ubers",
  "Untraded deaths",
  "defenders per 10 min",
  "net per 10 min",
  "per 10 min",
  "per min",
  "teammates dead per 10 min",
  "seconds per 10 min",
  "win % per 10 min",
];

function walk(d, out = []) {
  for (const name of readdirSync(d)) {
    const p = join(d, name);
    if (statSync(p).isDirectory()) walk(p, out);
    else if (/\.(ts|tsx)$/.test(name)) out.push(p);
  }
  return out;
}

const keys = new Set(LITERALS);
// t("...") or tr("...") -- the alias used where `t` is a local name --
// allowing escaped quotes inside.
const call = /\b(?:t|tr|tx|k)\(\s*"((?:[^"\\]|\\.)*)"/g;
for (const file of walk(src)) {
  const text = readFileSync(file, "utf8");
  for (const m of text.matchAll(call)) keys.add(JSON.parse(`"${m[1]}"`));
}

// The .lang format, as ui/src/lib/langfile.ts reads and writes it; keep the
// two in step.
const STR = String.raw`"(?:[^"\\]|\\.)*"`;
const ENTRY = new RegExp(`^(${STR})\\s*=\\s*(${STR})$`);
const HEADER = new RegExp(`^(language|locale)\\s*=\\s*(${STR})$`);

function parseLang(text, name) {
  const out = { name: null, locale: null, entries: {} };
  text.replace(/^\uFEFF/, "").split(/\r?\n/).forEach((raw, i) => {
    const line = raw.trim();
    if (!line || line.startsWith("#")) return;
    const h = HEADER.exec(line);
    if (h) {
      out[h[1] === "language" ? "name" : "locale"] = JSON.parse(h[2]);
      return;
    }
    const m = ENTRY.exec(line);
    if (!m) throw new Error(`${name} line ${i + 1} is not "English" = "Translation": ${line}`);
    out.entries[JSON.parse(m[1])] = JSON.parse(m[2]);
  });
  return out;
}

const INTRO = `# One line per piece of text: the English on the left, this language on the
# right. Change only the right-hand side. An empty right side ("") shows the
# English. Keep {placeholders} like {0} or {name}: the app fills them with
# numbers and names, and they can move to wherever the sentence needs them.
#
# To try a change: put this file in the app's lang folder (Settings, Language,
# "Open the language folder") and press Reload. Send the file on Discord to
# have it included for everyone.`;

// en.lang is not a translation: both sides start as the same English, and a
// changed right-hand side is a correction to the English itself.
const EN_INTRO = `# One line per piece of text: the English the app says now on the left, and
# the same English on the right. To correct a line, change only the
# right-hand side. Lines left as they are change nothing. Keep {placeholders}
# like {0} or {name}: the app fills them with numbers and names.
#
# To try a change: put this file in the app's lang folder (Settings, Language,
# "Show the language folder") and press Reload. Send the file on Discord and
# the corrections go into the app itself, for every language.`;

function writeLang(name, locale, entries, intro = INTRO) {
  const keys = Object.keys(entries).sort((a, b) => a.localeCompare(b));
  const body = keys.map((k) => `${JSON.stringify(k)} = ${JSON.stringify(entries[k] ?? "")}`);
  return [`# Flashwave.tf: ${name}`, "#", intro, "", `language = ${JSON.stringify(name)}`, `locale = ${JSON.stringify(locale)}`, "", ...body, ""].join("\n");
}

let bad = 0;
const sorted = [...keys].sort((a, b) => a.localeCompare(b));
for (const name of readdirSync(dir).filter((n) => n.endsWith(".lang") && n !== "template.lang" && n !== "en.lang").sort()) {
  const path = join(dir, name);
  const file = parseLang(readFileSync(path, "utf8"), name);
  const table = file.entries;
  const missing = [...keys].filter((k) => !(k in table));
  const stale = Object.keys(table).filter((k) => !keys.has(k));
  const done = [...keys].filter((k) => table[k]).length;
  console.log(`${name.padEnd(9)} ${done}/${keys.size} translated, ${missing.length} missing, ${stale.length} stale`);
  for (const k of stale) console.log(`  stale: ${JSON.stringify(k)}${table[k] ? "  (had a translation)" : ""}`);
  if (write) {
    const next = {};
    for (const k of sorted) next[k] = table[k] ?? "";
    writeFileSync(path, writeLang(file.name ?? name.replace(/\.lang$/, ""), file.locale ?? "en-GB", next));
  } else if (missing.length || stale.length) {
    bad += 1;
  }
}
// A blank file to start a new language from: every key, nothing translated.
if (write) {
  writeFileSync(join(dir, "template.lang"), writeLang("New language", "en-GB", Object.fromEntries(sorted.map((k) => [k, ""]))));
}
// English to correct: every key, both sides the same.
const english = writeLang("English", "en-GB", Object.fromEntries(sorted.map((k) => [k, k])), EN_INTRO);
if (write) {
  writeFileSync(join(dir, "en.lang"), english);
} else {
  let current = "";
  try {
    current = readFileSync(join(dir, "en.lang"), "utf8");
  } catch {
    // Missing: reported below.
  }
  if (current !== english) {
    console.log("en.lang   out of step with the code");
    bad += 1;
  }
}
if (!write && bad) console.log("\nRun `npm run i18n -- --write` to bring the files up to date.");
