// The type scale's guard (docs: "Flashwave.tf typography"). Fails when a
// stylesheet or a component sets text outside the scale: a font size,
// letter-spacing, line height, weight or family written as a literal
// instead of one of the tokens in ui/src/index.css's :root. Without it the
// scale drifts back: the sweep of 25 October 2026 found about 60 sizes,
// most of them 0.01-0.02 rem apart.
//
//   node scripts/check-type.mjs        check, list what is off, exit 1 if any
//
// Runs in `npm run lint` and `npm run build` in ui/, so in CI and every
// release. A value that is meant to be off the scale -- a glyph drawn as
// text, SVG chart text sized in viewBox units -- carries a comment on the
// same line saying why:
//
//   .mx-score { font: 700 4.2rem/1 var(--font-display); } /* type-ok: the match header's scoreline */
//
// Allowed everywhere: var(...), inherit, em sizes (they follow the text they
// sit in), line-height 1 and normal, letter-spacing 0 and normal.

import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const src = join(root, "ui", "src");

function walk(dir, out = []) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) walk(p, out);
    else if (/\.(css|tsx)$/.test(name)) out.push(p);
  }
  return out;
}

const SIZE_OK = /^(var\(--fs-[a-z0-9-]+\)|inherit|[\d.]+em|\d+%)$/;
const WEIGHTS = new Set(["400", "600", "700", "normal", "bold", "inherit"]);
const problems = [];

/** One declaration's value, checked; `what` names the property. */
function check(file, line, what, value) {
  const v = value.trim().replace(/\s*!important$/, "");
  if (what === "font-size" && !SIZE_OK.test(v)) return `font-size ${v}: use a --fs-* token`;
  if (what === "letter-spacing" && !/^(var\(--ls-[a-z-]+\)|normal|0|inherit)$/.test(v)) return `letter-spacing ${v}: use --ls-heading or --ls-caps`;
  if (what === "line-height" && !/^(var\(--lh-[a-z-]+\)|1|normal|inherit)$/.test(v)) return `line-height ${v}: use --lh-tight, --lh-snug or --lh-body`;
  if (what === "font-weight" && !WEIGHTS.has(v)) return `font-weight ${v}: 400, 600 or 700 (500 and 800 do not render)`;
  if (what === "font-family" && !/^(var\(--font-[a-z]+\)|inherit)$/.test(v)) return `font-family ${v}: use --font-display, --font-text or --font-mono`;
  return null;
}

/** `font: [weight] size[/line-height] family`, in its parts. */
function checkShorthand(file, line, value) {
  const v = value.trim();
  if (v === "inherit") return [];
  const m = v.match(/^(?:(\d{3}|normal|bold)\s+)?(var\(--fs-[a-z0-9-]+\)|[^\s/]+)\s*(?:\/\s*(var\(--lh-[a-z-]+\)|[^\s]+)\s+)?(.+)$/);
  if (!m) return [`font: ${v}: write it as weight, a --fs-* size, an optional / line height, and a --font-* family`];
  const out = [];
  if (m[1]) out.push(check(file, line, "font-weight", m[1]));
  out.push(check(file, line, "font-size", m[2]));
  if (m[3]) out.push(check(file, line, "line-height", m[3]));
  out.push(check(file, line, "font-family", m[4]));
  return out.filter(Boolean);
}

for (const file of walk(src)) {
  const text = readFileSync(file, "utf8");
  const rel = relative(root, file).replaceAll("\\", "/");
  const lines = text.split(/\r?\n/);
  if (file.endsWith(".css")) {
    // Rule by rule: uppercase only in the display face (Segoe UI in capitals
    // reads as shouting), and the display face at 700.
    const lineOf = (at) => text.slice(0, at).split("\n").length;
    // Comments blanked to spaces, so positions and line numbers still match.
    const bare = text.replace(/\/\*[\s\S]*?\*\//g, (c) => c.replace(/[^\n]/g, " "));
    for (const m of bare.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
      const [, sel, body] = m;
      const n = lineOf(m.index + sel.length);
      if (/type-ok:/.test(lines[n - 1] ?? "") || sel.trim().startsWith("@")) continue;
      const display = /var\(--font-display\)/.test(body);
      const family = /font-family\s*:|(?<![-\w])font\s*:\s*(?!inherit)/.test(body);
      if (/text-transform\s*:\s*uppercase/.test(body) && family && !display)
        problems.push(`${rel}:${n}  uppercase in the text face: uppercase is for --font-display`);
      if (/text-transform\s*:\s*uppercase/.test(body) && !family)
        problems.push(`${rel}:${n}  uppercase with no family: give it font-family: var(--font-display)`);
      const w = body.match(/font-weight\s*:\s*(\d{3})|(?<![-\w])font\s*:\s*(\d{3})\s+\S+\s+var\(--font-display\)/);
      if (display && w && (w[1] ?? w[2]) !== "700")
        problems.push(`${rel}:${n}  the display face at ${w[1] ?? w[2]}: it is set at 700`);
    }
    lines.forEach((ln, i) => {
      if (/type-ok:/.test(ln)) return;
      // Comments out, so a value quoted in one is not checked.
      const code = ln.replace(/\/\*.*?\*\//g, "");
      for (const m of code.matchAll(/(?<![-\w])(font-size|letter-spacing|line-height|font-weight|font-family|font)\s*:\s*([^;}]+)/g)) {
        // :root defines the tokens themselves.
        if (/^\s*--/.test(ln)) continue;
        const found = m[1] === "font" ? checkShorthand(file, i + 1, m[2]) : [check(file, i + 1, m[1], m[2])].filter(Boolean);
        for (const p of found) problems.push(`${rel}:${i + 1}  ${p}`);
      }
    });
  } else {
    // Components: an inline style's fontSize, letterSpacing, lineHeight or
    // fontWeight written as a number or a literal string.
    lines.forEach((ln, i) => {
      if (/type-ok:/.test(ln)) return;
      for (const m of ln.matchAll(/\b(fontSize|letterSpacing|lineHeight|fontWeight|fontFamily)\s*:\s*("[^"]*"|'[^']*'|[\d.]+)/g)) {
        const v = m[2].replace(/^["']|["']$/g, "");
        if (/^var\(--(fs|ls|lh|font)-/.test(v) || (m[1] === "fontWeight" && WEIGHTS.has(v)) || (m[1] === "lineHeight" && v === "1")) continue;
        problems.push(`${rel}:${i + 1}  ${m[1]} ${v}: use a token (var(--fs-*), var(--ls-*), var(--lh-*), var(--font-*))`);
      }
    });
  }
}

if (problems.length) {
  console.error(`Type scale: ${problems.length} value(s) off the scale (docs: "Flashwave.tf typography").`);
  for (const p of problems) console.error(`  ${p}`);
  console.error(`A value meant to be off the scale takes a /* type-ok: why */ comment on its line.`);
  process.exit(1);
}
console.log("Type scale: every size, spacing, line height, weight and family is on the scale.");
