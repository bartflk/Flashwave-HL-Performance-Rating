import { test, beforeEach } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { isValidElement, type ReactElement } from "react";
import { parseLang, writeLang } from "../src/lib/langfile.ts";
import { allLanguages, applyUserFiles, languageFile, locale, setLanguage, t, tx } from "../src/lib/i18n.ts";

const LANG_DIR = join(import.meta.dirname, "..", "..", "lang");
const placeholders = (s: string) => new Set([...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]));

beforeEach(() => {
  localStorage.clear();
  applyUserFiles([]);
  setLanguage("en");
});

// ---- langfile ---------------------------------------------------------------

test("parseLang: headers, entries, comments, blank lines, BOM and CRLF", () => {
  const text = '﻿# a comment\r\nlanguage = "Français"\r\nlocale = "fr-FR"\r\n\r\n"Matches" = "Matchs"\r\n"Loading…" = ""\r\n';
  const f = parseLang(text);
  assert.equal(f.name, "Français");
  assert.equal(f.locale, "fr-FR");
  assert.deepEqual(f.entries, { Matches: "Matchs", "Loading…": "" });
  assert.deepEqual(f.errors, []);
});

test("parseLang: quotes, backslashes, newlines and any script survive", () => {
  const entries = { 'say "hi"': 'dis « salut » \\ ça', "line1\nline2": "строка\tтаб", "日本語": "🙂" };
  const text = writeLang("Test", "xx-XX", entries);
  const f = parseLang(text);
  assert.deepEqual(f.entries, entries);
  assert.deepEqual(f.errors, []);
});

test("parseLang: a bad line is reported with its number and the rest still reads", () => {
  const f = parseLang('"a" = "b"\nthis is not a line\n"c" = "bad \\q escape"\n"d" = "e"');
  assert.deepEqual(f.entries, { a: "b", d: "e" });
  assert.equal(f.errors.length, 2);
  assert.match(f.errors[0], /^line 2:/);
  assert.match(f.errors[1], /^line 3:/);
});

test("parseLang: a long bad line is cut in the message", () => {
  const f = parseLang("x".repeat(200));
  assert.equal(f.errors.length, 1);
  assert.ok(f.errors[0].length < 90);
});

test("parseLang: empty text", () => {
  const f = parseLang("");
  assert.deepEqual(f, { name: null, locale: null, entries: {}, errors: [] });
});

test("parseLang: a later duplicate key wins", () => {
  assert.deepEqual(parseLang('"a" = "1"\n"a" = "2"').entries, { a: "2" });
});

test("writeLang then parseLang round-trips the language name and locale", () => {
  const f = parseLang(writeLang('Es "ñ"', "es-ES", { b: "2", a: "1" }));
  assert.equal(f.name, 'Es "ñ"');
  assert.equal(f.locale, "es-ES");
  assert.deepEqual(f.entries, { a: "1", b: "2" });
});

// ---- the shipped files ------------------------------------------------------

const shipped = readdirSync(LANG_DIR).filter((f) => f.endsWith(".lang"));

for (const file of shipped) {
  test(`lang/${file}: reads without errors`, () => {
    const f = parseLang(readFileSync(join(LANG_DIR, file), "utf8"));
    assert.deepEqual(f.errors, []);
  });
}

for (const file of shipped.filter((f) => !["en.lang", "template.lang"].includes(f))) {
  test(`lang/${file}: a translation never invents a placeholder the English lacks`, () => {
    const f = parseLang(readFileSync(join(LANG_DIR, file), "utf8"));
    const bad: string[] = [];
    for (const [key, value] of Object.entries(f.entries)) {
      if (!value) continue;
      const have = placeholders(key);
      for (const p of placeholders(value)) if (!have.has(p)) bad.push(`${key} -> ${value}`);
    }
    assert.deepEqual(bad, []);
  });
}

// ---- t() --------------------------------------------------------------------

test("t: null and undefined are an empty string", () => {
  assert.equal(t(null), "");
  assert.equal(t(undefined), "");
});

test("t: English falls through, placeholders are filled, repeated ones too", () => {
  assert.equal(t("{0} and {0} and {n}", { "0": "a", n: 3 }), "a and a and 3");
  assert.equal(t("no placeholders"), "no placeholders");
  assert.equal(t("{x} stays", {}), "{x} stays");
});

test("t: a value containing a placeholder-looking text is not filled twice", () => {
  // A team called "{b}" must show as "{b}", not as whatever {b} is.
  assert.equal(t("{a}", { a: "{b}", b: "!" }), "{b}");
  assert.equal(t("{0} vs {1}", { "0": "{1}", "1": "x" }), "{1} vs x");
});

test("t: a number 0 is shown, not skipped", () => {
  assert.equal(t("{n} games", { n: 0 }), "0 games");
});

test("t: picks the chosen language and falls back to the English for a missing line", () => {
  applyUserFiles([{ id: "xx", text: 'language = "Testish"\nlocale = "de-DE"\n"Matches" = "Spiele"\n"Players" = ""\n' }]);
  setLanguage("xx");
  assert.equal(t("Matches"), "Spiele");
  assert.equal(t("Players"), "Players"); // empty right-hand side: not translated yet
  assert.equal(t("Something never translated"), "Something never translated");
  assert.equal(locale(), "de-DE");
});

test("t: a translation can move the placeholders", () => {
  applyUserFiles([{ id: "xx", text: '"{0} of {1}" = "{1} über {0}"' }]);
  setLanguage("xx");
  assert.equal(t("{0} of {1}", { "0": "a", "1": "b" }), "b über a");
});

test("applyUserFiles: a language that vanishes falls back to English", () => {
  applyUserFiles([{ id: "xx", text: '"Matches" = "Spiele"' }]);
  setLanguage("xx");
  assert.equal(t("Matches"), "Spiele");
  applyUserFiles([]);
  assert.equal(t("Matches"), "Matches");
  assert.equal(locale(), "en-GB");
});

test("applyUserFiles: counts only lines that differ from what ships, flags added languages", async () => {
  const { userLanguageFiles } = await import("../src/lib/i18n.ts");
  const fr = parseLang(readFileSync(join(LANG_DIR, "fr.lang"), "utf8"));
  const [key, value] = Object.entries(fr.entries).find(([, v]) => v) ?? ["", ""];
  applyUserFiles([
    { id: "fr", text: `"${key.replace(/[\\"]/g, "\\$&")}" = ${JSON.stringify(value)}\n"Zzz not shipped" = "Zzz"` },
    { id: "pl", text: 'language = "Polski"\n"Matches" = "Mecze"' },
  ]);
  const files = userLanguageFiles();
  const frInfo = files.find((f) => f.id === "fr")!;
  const pl = files.find((f) => f.id === "pl")!;
  assert.equal(frInfo.lines, 1); // the untouched shipped line is not counted; the new one is
  assert.equal(frInfo.added, false);
  assert.equal(pl.added, true);
  assert.equal(pl.lines, 1);
  assert.ok(allLanguages().some((l) => l.id === "pl" && l.name === "Polski"));
});

test("languageFile: English is the blank template, other languages carry what is shown", () => {
  const en = parseLang(languageFile("en"));
  assert.ok(Object.keys(en.entries).length > 100);
  assert.ok(Object.values(en.entries).every((v) => v === ""));
  const fr = parseLang(languageFile("fr"));
  assert.equal(fr.name, "Français");
  assert.deepEqual(Object.keys(fr.entries).sort(), Object.keys(en.entries).sort());
});

// ---- tx() -------------------------------------------------------------------

function flatten(node: unknown): unknown[] {
  assert.ok(isValidElement(node));
  return (node as ReactElement<{ children?: unknown }>).props.children === undefined ? [] : [(node as ReactElement<{ children?: unknown }>).props.children].flat();
}

test("tx: pieces are dropped in where their placeholders sit", () => {
  const out = flatten(tx("Deleted {n} demos, freeing {size}.", { n: 3, size: "1 GB" }));
  assert.deepEqual(out, ["Deleted ", 3, " demos, freeing ", "1 GB", "."]);
});

test("tx: a placeholder with no piece is left as written; a leftover piece is ignored", () => {
  assert.deepEqual(flatten(tx("{a} and {b}", { a: "x" })), ["x", " and ", "{b}"]);
  assert.deepEqual(flatten(tx("plain", { a: "x" })), ["plain"]);
});

test("tx: a translation that omits a placeholder just does not show it", () => {
  applyUserFiles([{ id: "xx", text: '"{n} kill{s}" = "{n} убийств"' }]);
  setLanguage("xx");
  assert.deepEqual(flatten(tx("{n} kill{s}", { n: 2, s: "s" })), [2, " убийств"]);
});

test("tx: a falsy piece (0) is kept", () => {
  assert.deepEqual(flatten(tx("{n}", { n: 0 })), [0]);
});
