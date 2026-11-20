import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const i18nPath = path.join(root, "src", "i18n.ts");
const srcRoot = path.join(root, "src");

const LOCALES = ["zh-CN", "zh-TW", "en", "ja", "ko"];

/** Extract object keys for each locale block in messages. */
function extractLocaleKeys(source) {
  const localeKeys = new Map();

  for (const locale of LOCALES) {
    // Locale keys may be quoted ("zh-CN") or bare identifiers (en, ja, ko).
    const escaped = locale.replace(/-/g, "\\-");
    const startRe = new RegExp(`(?:^|[\\s,{])(?:"${escaped}"|${escaped}):\\s*\\{`, "m");
    const startMatch = startRe.exec(source);
    if (!startMatch) {
      localeKeys.set(locale, new Set());
      continue;
    }

    let i = startMatch.index + startMatch[0].length;
    let depth = 1;
    const keys = new Set();

    while (i < source.length && depth > 0) {
      const ch = source[i];

      if (ch === '"' || ch === "'" || ch === "`") {
        const quote = ch;
        i++;
        while (i < source.length) {
          if (source[i] === "\\") {
            i += 2;
            continue;
          }
          if (source[i] === quote) {
            i++;
            break;
          }
          i++;
        }
        continue;
      }

      if (ch === "/" && source[i + 1] === "/") {
        i += 2;
        while (i < source.length && source[i] !== "\n") i++;
        continue;
      }
      if (ch === "/" && source[i + 1] === "*") {
        i += 2;
        while (i < source.length - 1 && !(source[i] === "*" && source[i + 1] === "/")) i++;
        i += 2;
        continue;
      }

      if (ch === "{") {
        depth++;
        i++;
        continue;
      }
      if (ch === "}") {
        depth--;
        i++;
        continue;
      }

      // Top-level key inside this locale object
      if (depth === 1) {
        const rest = source.slice(i);
        const keyMatch = rest.match(/^([A-Za-z_][A-Za-z0-9_]*)\s*:/);
        if (keyMatch) {
          keys.add(keyMatch[1]);
          i += keyMatch[0].length;
          continue;
        }
      }

      i++;
    }

    localeKeys.set(locale, keys);
  }

  return localeKeys;
}

function walkFiles(dir, out = []) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    if (entry.name === "node_modules" || entry.name === "dist") continue;
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) walkFiles(full, out);
    else if (/\.(ts|vue)$/.test(entry.name) && entry.name !== "i18n.ts") out.push(full);
  }
  return out;
}

/** Collect i18n key references from source files. */
function collectUsedKeys(files) {
  const used = new Set();
  const patterns = [
    /\bt\(\s*['"]([A-Za-z_][A-Za-z0-9_]*)['"]\s*[,)]/g,
    /\bi18n\.global\.t\(\s*['"]([A-Za-z_][A-Za-z0-9_]*)['"]\s*[,)]/g,
    /\bkeypath\s*=\s*['"]([A-Za-z_][A-Za-z0-9_]*)['"]/g,
    /\b(?:labelKey|nameKey|hintKey|descKey|descriptionKey)\s*[:=]\s*['"]([A-Za-z_][A-Za-z0-9_]*)['"]/g,
  ];

  for (const file of files) {
    const text = fs.readFileSync(file, "utf8");
    for (const re of patterns) {
      re.lastIndex = 0;
      let m;
      while ((m = re.exec(text)) !== null) used.add(m[1]);
    }
  }
  return used;
}

const source = fs.readFileSync(i18nPath, "utf8");
const localeKeys = extractLocaleKeys(source);

const baseLocale = "zh-CN";
const baseKeys = localeKeys.get(baseLocale) || new Set();
let keySetsDiffer = false;

console.log(`Locales: ${LOCALES.join(", ")}`);
console.log(`${baseLocale} keys: ${baseKeys.size}`);

for (const locale of LOCALES) {
  if (locale === baseLocale) continue;
  const keys = localeKeys.get(locale) || new Set();
  const missing = [...baseKeys].filter((k) => !keys.has(k)).sort();
  const extra = [...keys].filter((k) => !baseKeys.has(k)).sort();
  if (missing.length || extra.length) {
    keySetsDiffer = true;
    console.error(`\n[${locale}] key set differs from ${baseLocale}:`);
    if (missing.length) console.error(`  missing (${missing.length}): ${missing.join(", ")}`);
    if (extra.length) console.error(`  extra (${extra.length}): ${extra.join(", ")}`);
  } else {
    console.log(`[${locale}] identical key set (${keys.size})`);
  }
}

const files = walkFiles(srcRoot);
const used = collectUsedKeys(files);
const unused = [...baseKeys].filter((k) => !used.has(k)).sort();
const missingInI18n = [...used].filter((k) => !baseKeys.has(k)).sort();

console.log(`\nScanned ${files.length} source files for i18n usage.`);
console.log(`Referenced keys: ${used.size}`);
if (missingInI18n.length) {
  console.warn(
    `Used but missing in ${baseLocale} (${missingInI18n.length}): ${missingInI18n.join(", ")}`,
  );
}
if (unused.length) {
  console.log(`Unused keys (${unused.length}, not failing):`);
  for (const k of unused) console.log(`  - ${k}`);
} else {
  console.log("No unused keys.");
}

if (keySetsDiffer) {
  console.error("\nFAIL: locale key sets are not identical.");
  process.exit(1);
}

console.log("\nOK: all locales have identical key sets.");
