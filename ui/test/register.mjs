import { register } from "node:module";

register("./hooks.mjs", import.meta.url);

// A browser's localStorage, in memory, so modules that read a saved choice
// at load behave as they do in the app. A test resets it with `localStorage.clear()`.
const data = new Map();
globalThis.localStorage = {
  getItem: (k) => (data.has(k) ? data.get(k) : null),
  setItem: (k, v) => void data.set(k, String(v)),
  removeItem: (k) => void data.delete(k),
  clear: () => data.clear(),
  get length() {
    return data.size;
  },
  key: (i) => [...data.keys()][i] ?? null,
};
globalThis.document = { documentElement: { lang: "en", style: { setProperty() {} } } };
