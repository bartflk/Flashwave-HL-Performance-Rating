import { createElement, Fragment, useSyncExternalStore, type ReactNode } from "react";
import { parseLang, writeLang } from "./langfile";

/**
 * Translations (Q13 — tenshi, with boSe on French and obi on Portuguese and
 * Spanish).
 *
 * **The English text is the key.** `t("Matches")` looks "Matches" up in the
 * chosen language's file and falls back to the English when there is no
 * entry. That is the whole design, and it is chosen for the volunteers
 * rather than for the code:
 *
 * - A translator sees exactly the sentence they are translating, not an
 *   invented key like `nav.matches` they have to find on screen to
 *   understand.
 * - A missing translation is never a blank or a key leaking onto the page:
 *   it is the English, which is what the app said yesterday.
 * - No string has to be moved to a table before it can be translated. It is
 *   wrapped where it is, which matters while the screens are still moving —
 *   the plan's warning about Q13 is that every string moved twice is a
 *   string translated twice.
 *
 * The cost is that fixing a typo in the English orphans its translations.
 * `npm run i18n` lists every key and every file's missing and stale ones,
 * so that shows up as a diff rather than a silent regression.
 *
 * The choice is per machine, like the theme, and lives in localStorage for
 * the same reason: it has to be readable before the first paint.
 *
 * Each language is a plain `.lang` file (see ./langfile.ts). The shipped
 * ones are built in from `lang/`; files in the user's own lang folder are
 * laid over them at start and on Reload, so a player can fix a line, see it
 * straight away, and send the file back to be shipped for everyone.
 */

/**
 * The built-in translations: `lang/*.lang` at the top of the repository,
 * the same files volunteers edit and send back, bundled in at build time.
 * `template.lang` is the blank one a new language starts from, and
 * `en.lang` is the English written out for correcting (both sides the
 * same); neither is a language to add.
 */
const BUILT_IN = import.meta.glob("../../../lang/*.lang", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

export type Language = string;

export type LanguageInfo = {
  id: Language;
  /** The language's own name, as its file gives it. */
  name: string;
  /** BCP 47 tag for dates and numbers. */
  locale: string;
};

// The shipped languages keep this order; any a user file adds follow them.
const ORDER = ["en", "fr", "pt", "es", "ru"];
const rank = (id: string) => (ORDER.includes(id) ? ORDER.indexOf(id) : ORDER.length);

const BASE_TABLES: Record<Language, Record<string, string>> = {};
const BASE_LANGUAGES: LanguageInfo[] = [{ id: "en", name: "English", locale: "en-GB" }];
for (const [path, text] of Object.entries(BUILT_IN)) {
  const id = path.split("/").pop()!.replace(/\.lang$/, "");
  if (id === "template" || id === "en") continue;
  const file = parseLang(text);
  BASE_TABLES[id] = file.entries;
  BASE_LANGUAGES.push({ id, name: file.name ?? id, locale: file.locale ?? "en-GB" });
}

let tables: Record<Language, Record<string, string>> = { ...BASE_TABLES };
let languages: LanguageInfo[] = sorted(BASE_LANGUAGES);

function sorted(list: LanguageInfo[]): LanguageInfo[] {
  return [...list].sort((a, b) => rank(a.id) - rank(b.id) || a.name.localeCompare(b.name));
}

/** Every language on offer: the shipped ones, then any a user file adds. */
export function allLanguages(): LanguageInfo[] {
  return languages;
}

const KEY = "hl.language";

function load(): Language {
  try {
    return localStorage.getItem(KEY) ?? "en";
  } catch {
    // Storage can be unavailable; English is always there.
    return "en";
  }
}

// May name a language only a user file adds. Until the files are read that
// shows English, and if no file adds it, it falls back to English for good.
let current: Language = load();
let snapshot = { current, version: 0 };
let listeners: Array<() => void> = [];

function emit() {
  snapshot = { current, version: snapshot.version + 1 };
  listeners.forEach((f) => f());
}

export function setLanguage(l: Language) {
  current = l;
  try {
    localStorage.setItem(KEY, l);
  } catch {
    // Not remembered across restarts, but the page still changes now.
  }
  document.documentElement.lang = l;
  emit();
}

/** What the app made of one file in the user's lang folder, for Settings. */
export type UserLanguageFile = {
  id: Language;
  name: string;
  /** Lines that differ from what the app ships: 0 for an untouched copy. */
  lines: number;
  /** True when it is a language the app does not ship. */
  added: boolean;
  errors: string[];
};

let userFiles: UserLanguageFile[] = [];

export function userLanguageFiles(): UserLanguageFile[] {
  return userFiles;
}

/**
 * Lay the user's `.lang` files over the built-in translations: a line in
 * `fr.lang` there beats the shipped French, and `pl.lang` adds Polish. Each
 * call starts again from the built-in files, so a line deleted from a user
 * file goes back to the shipped translation on Reload. Empty right-hand
 * sides are skipped, so a full copy of a language only changes what was
 * actually filled in.
 */
export function applyUserFiles(files: Array<{ id: string; text: string }>) {
  tables = { ...BASE_TABLES };
  const list = BASE_LANGUAGES.map((l) => ({ ...l }));
  userFiles = [];
  for (const f of files) {
    const parsed = parseLang(f.text);
    const entries = Object.fromEntries(Object.entries(parsed.entries).filter(([, v]) => v !== ""));
    const known = list.find((l) => l.id === f.id);
    if (known) {
      if (parsed.name) known.name = parsed.name;
      if (parsed.locale) known.locale = parsed.locale;
    } else {
      list.push({ id: f.id, name: parsed.name ?? f.id, locale: parsed.locale ?? "en-GB" });
    }
    const base = BASE_TABLES[f.id] ?? {};
    tables[f.id] = { ...(tables[f.id] ?? {}), ...entries };
    // English ships as itself on both sides: a line differs when it no
    // longer matches its own key.
    const shipped = (key: string) => (f.id === "en" ? key : base[key]);
    userFiles.push({
      id: f.id,
      name: parsed.name ?? known?.name ?? f.id,
      lines: Object.entries(entries).filter(([k, v]) => shipped(k) !== v).length,
      added: !known,
      errors: parsed.errors,
    });
  }
  languages = sorted(list);
  if (!languages.some((l) => l.id === current)) current = "en";
  document.documentElement.lang = current;
  emit();
}

/**
 * A language as a `.lang` file for someone to edit: every line the app has,
 * translated or not, with what this machine currently shows. For English it
 * is the blank template a new language starts from.
 */
export function languageFile(id: Language): string {
  const keys = new Set<string>();
  for (const table of Object.values(BASE_TABLES)) for (const k of Object.keys(table)) keys.add(k);
  if (id === "en") return writeLang("New language", "en-GB", Object.fromEntries([...keys].map((k) => [k, ""])));
  const info = languages.find((l) => l.id === id);
  const table = tables[id] ?? {};
  return writeLang(info?.name ?? id, info?.locale ?? "en-GB", Object.fromEntries([...keys].map((k) => [k, table[k] ?? ""])));
}

/**
 * The text in the chosen language. `{name}`-style placeholders are filled
 * from `vars` *after* the lookup, so a translation can move them to wherever
 * its grammar wants them.
 */
export function t(english: string | null | undefined, vars?: Record<string, string | number>): string {
  // A label that may be missing renders as nothing, the same as before it
  // was wrapped.
  if (english == null) return "";
  // English has no table unless a user file fixes some of it.
  const table = tables[current];
  let s = table?.[english] || english;
  if (vars) {
    // One pass, so a value that itself looks like a placeholder (a team
    // called "{0}") is shown as it is and not filled in a second time.
    s = s.replace(/\{([^{}]+)\}/g, (whole, k: string) => (Object.hasOwn(vars, k) ? String(vars[k]) : whole));
  }
  return s;
}

/**
 * A sentence with live pieces in it: numbers, names, a bolded word, a link.
 *
 * `tx("Deleted {n} demos, freeing {size}.", { n: <strong>{n}</strong>, size })`
 * translates the *whole sentence* and then drops each piece in where its
 * `{name}` sits. That is the difference from translating the fragments
 * around a value one at a time: Russian, like most languages, does not keep
 * English word order, and a translation can only move a number if the number
 * is part of the sentence it is translating.
 *
 * A placeholder the translation leaves out is simply not shown, which is how
 * English-only grammar -- the `{s}` on the end of "kill{s}" -- is dropped by a
 * language that says it another way.
 */
export function tx(english: string, parts: Record<string, ReactNode>): ReactNode {
  const table = tables[current];
  const s = table?.[english] || english;
  const out: ReactNode[] = [];
  let last = 0;
  for (const m of s.matchAll(/\{(\w+)\}/g)) {
    if (m.index! > last) out.push(s.slice(last, m.index));
    out.push(m[1] in parts ? parts[m[1]] : m[0]);
    last = m.index! + m[0].length;
  }
  if (last < s.length) out.push(s.slice(last));
  return createElement(Fragment, null, ...out);
}

/**
 * Marks English for translation without translating it.
 *
 * For text defined once and shown later through a variable -- a table of
 * stat definitions, a list of chart metrics. Translating there would freeze
 * the language at import; marking it lets `npm run i18n` find the text, and
 * the place that renders it calls `t(label)`.
 */
export const k = (english: string): string => english;

/** The BCP 47 tag for dates and numbers in the chosen language. */
export function locale(): string {
  return languages.find((l) => l.id === current)?.locale ?? "en-GB";
}

/** Re-render when the language changes, and return the current one. */
export function useLanguage(): Language {
  return useSyncExternalStore(
    (l) => {
      listeners.push(l);
      return () => {
        listeners = listeners.filter((x) => x !== l);
      };
    },
    // The snapshot changes on a switch *and* when user files are read, so
    // a fixed line shows up without switching away and back.
    () => snapshot,
  ).current;
}
