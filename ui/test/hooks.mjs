// Module hooks for `node --test`: the app's source is written for Vite, so
// Node needs a few small helps to load it. No dependency is added: the
// TypeScript compiler is already in devDependencies.
//
//  - extensionless imports ("./i18n") resolve to .ts or .tsx, as Vite does;
//  - .tsx files (JSX, which Node does not strip) go through
//    ts.transpileModule;
//  - `import.meta.glob`, which only Vite has, is filled in with the real
//    lang/*.lang files, and api/client (Tauri, import.meta.env) is a stub.
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { fileURLToPath, pathToFileURL } from "node:url";
import { dirname, join } from "node:path";
import ts from "typescript";

const here = dirname(fileURLToPath(import.meta.url));
const stub = pathToFileURL(join(here, "stub-client.ts")).href;

export async function resolve(specifier, context, next) {
  if (/(^|\/)api\/client$/.test(specifier)) return { url: stub, shortCircuit: true };
  if (specifier.startsWith(".") && context.parentURL) {
    const path = fileURLToPath(new URL(specifier, context.parentURL));
    for (const ext of [".ts", ".tsx"]) {
      if (existsSync(path + ext)) return { url: pathToFileURL(path + ext).href, shortCircuit: true };
    }
  }
  return next(specifier, context);
}

function langFiles() {
  const dir = join(here, "..", "..", "lang");
  const out = {};
  for (const f of readdirSync(dir)) if (f.endsWith(".lang")) out[`../../../lang/${f}`] = readFileSync(join(dir, f), "utf8");
  return out;
}

export async function load(url, context, next) {
  if (url.endsWith(".tsx")) {
    const src = readFileSync(fileURLToPath(url), "utf8");
    const out = ts.transpileModule(src, { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2023, jsx: ts.JsxEmit.ReactJSX } });
    return { format: "module", source: out.outputText, shortCircuit: true };
  }
  if (url.endsWith("/lib/i18n.ts")) {
    const src = readFileSync(fileURLToPath(url), "utf8").replace(/import\.meta\.glob\([\s\S]*?\}\)/, JSON.stringify(langFiles()));
    return { format: "module-typescript", source: src, shortCircuit: true };
  }
  return next(url, context);
}
