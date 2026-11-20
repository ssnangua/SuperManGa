/** Filename chapter-match rules (multi-rule). */

export type ChapterFilenameRule = {
  id: string;
  pattern: string;
  enabled: boolean;
  examples: string[];
  builtIn: boolean;
};

export type ChapterExamplePreviewItem = {
  hit: boolean;
  /** Full example string (e.g. `1_01.jpg`). */
  text: string;
  /** Capture highlight range within `text` [start, end). */
  capture?: [number, number];
};

export type ChapterExamplesPreview =
  | { kind: "empty" }
  | { kind: "error"; message: string }
  | { kind: "ok"; items: ChapterExamplePreviewItem[] };

export const BUILTIN_CHAPTER_RULE_ID = "builtin-num-underscore";

export const DEFAULT_CHAPTER_RULE_PATTERN = "([\\d.-]+)_\\d+";

const DEFAULT_EXAMPLES = ["1_01.jpg", "12.5_03.png", "2-1_01.webp"];

export function createDefaultChapterRules(): ChapterFilenameRule[] {
  return [
    {
      id: BUILTIN_CHAPTER_RULE_ID,
      pattern: DEFAULT_CHAPTER_RULE_PATTERN,
      enabled: true,
      examples: [...DEFAULT_EXAMPLES],
      builtIn: true,
    },
  ];
}

export function getBuiltinRuleDefaults(
  id: string,
): { pattern: string; examples: string[] } | null {
  if (id === BUILTIN_CHAPTER_RULE_ID) {
    return {
      pattern: DEFAULT_CHAPTER_RULE_PATTERN,
      examples: [...DEFAULT_EXAMPLES],
    };
  }
  return null;
}

export function generateChapterRuleId(): string {
  return `rule-${Date.now()}-${Math.random().toString(36).slice(2, 10)}`;
}

export function cloneChapterRule(rule: ChapterFilenameRule): ChapterFilenameRule {
  return { ...rule, examples: [...rule.examples] };
}

export function cloneChapterRules(rules: ChapterFilenameRule[]): ChapterFilenameRule[] {
  return rules.map(cloneChapterRule);
}

function matchToTitle(match: RegExpMatchArray): string | null {
  const g1 = (match[1] ?? "").trim();
  if (g1) return g1;
  const full = (match[0] ?? "").trim();
  return full || null;
}

export function compileEnabledChapterRules(
  rules: ChapterFilenameRule[],
): { ok: true; regexes: RegExp[] } | { ok: false; error: string } {
  if (!Array.isArray(rules) || rules.length === 0) {
    return { ok: false, error: "empty" };
  }
  const regexes: RegExp[] = [];
  for (const rule of rules) {
    if (!rule.enabled) continue;
    const pattern = rule.pattern?.trim();
    if (!pattern) continue;
    try {
      regexes.push(new RegExp(pattern, "i"));
    } catch {
      return { ok: false, error: "invalid" };
    }
  }
  if (regexes.length === 0) return { ok: false, error: "noneEnabled" };
  return { ok: true, regexes };
}

/** First enabled rule that matches `name` wins; title from capture group 1. */
export function matchChapterTitle(name: string, rules: ChapterFilenameRule[]): string | null {
  const compiled = compileEnabledChapterRules(rules);
  if (!compiled.ok) return null;
  for (const re of compiled.regexes) {
    const match = name.match(re);
    if (!match) continue;
    const title = matchToTitle(match);
    if (title) return title;
  }
  return null;
}

function captureRange(
  line: string,
  match: RegExpMatchArray,
): [number, number] | undefined {
  if (match.index == null) return undefined;
  const indexed = match as RegExpMatchArray & {
    indices?: Array<[number, number] | undefined>;
  };
  const g1 = indexed.indices?.[1];
  if (g1) return g1;
  const title = (match[1] ?? "").trim();
  if (title) {
    const at = line.indexOf(title, match.index);
    if (at >= 0) return [at, at + title.length];
  }
  const full = indexed.indices?.[0];
  if (full) return full;
  return [match.index, match.index + match[0].length];
}

export function previewChapterExamples(
  pattern: string,
  examples: string[],
): ChapterExamplesPreview {
  const lines = examples.map((s) => s.trim()).filter(Boolean);
  if (lines.length === 0) return { kind: "empty" };
  let re: RegExp;
  try {
    // `d` enables match.indices for precise capture highlighting when supported.
    re = new RegExp(pattern.trim(), "id");
  } catch {
    try {
      re = new RegExp(pattern.trim(), "i");
    } catch {
      return { kind: "error", message: "invalid" };
    }
  }
  const items = lines.map((line): ChapterExamplePreviewItem => {
    const match = line.match(re);
    if (!match) return { hit: false, text: line };
    const title = matchToTitle(match);
    if (!title) return { hit: false, text: line };
    return { hit: true, text: line, capture: captureRange(line, match) };
  });
  return { kind: "ok", items };
}

export function normalizeLoadedChapterRules(raw: unknown): ChapterFilenameRule[] | null {
  if (!Array.isArray(raw) || raw.length === 0) return null;
  const out: ChapterFilenameRule[] = [];
  const seen = new Set<string>();
  for (const item of raw) {
    if (!item || typeof item !== "object") return null;
    const o = item as Record<string, unknown>;
    if (typeof o.id !== "string" || !o.id.trim()) return null;
    if (seen.has(o.id)) return null;
    seen.add(o.id);
    if (typeof o.pattern !== "string") return null;
    if (typeof o.enabled !== "boolean") return null;
    if (typeof o.builtIn !== "boolean") return null;
    if (!Array.isArray(o.examples)) return null;
    out.push({
      id: o.id,
      pattern: o.pattern,
      enabled: o.enabled,
      examples: o.examples.filter((x): x is string => typeof x === "string"),
      builtIn: o.builtIn,
    });
  }
  return out;
}

/** Load saved chapter rules, or fall back to built-in defaults. */
export function resolveChapterRules(rulesRaw: unknown): ChapterFilenameRule[] {
  return normalizeLoadedChapterRules(rulesRaw) ?? createDefaultChapterRules();
}
