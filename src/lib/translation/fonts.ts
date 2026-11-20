import { invoke } from "@tauri-apps/api/core";

/** Prefer CJK gothic / heiti faces for default overlay font. */
const HEITI_CANDIDATES = [
  "Microsoft YaHei",
  "微软雅黑",
  "Microsoft YaHei UI",
  "SimHei",
  "黑体",
  "Noto Sans CJK SC",
  "Source Han Sans SC",
  "PingFang SC",
  "Hiragino Sans GB",
  "WenQuanYi Micro Hei",
] as const;

let cachedSystemFonts: string[] | null = null;
let inflightSystemFonts: Promise<string[]> | null = null;

export function getCachedSystemFonts(): readonly string[] | null {
  return cachedSystemFonts;
}

/** Process-lifetime cache; concurrent callers share one invoke. */
export async function listSystemFontsCached(): Promise<string[]> {
  if (cachedSystemFonts) return cachedSystemFonts;
  if (inflightSystemFonts) return inflightSystemFonts;
  inflightSystemFonts = invoke<string[]>("list_system_fonts")
    .then((fonts) => {
      cachedSystemFonts = fonts;
      return fonts;
    })
    .finally(() => {
      inflightSystemFonts = null;
    });
  return inflightSystemFonts;
}

export function pickDefaultHeitiFont(fonts: readonly string[]): string {
  const set = new Map(fonts.map((f) => [f.toLowerCase(), f]));
  for (const name of HEITI_CANDIDATES) {
    const hit = set.get(name.toLowerCase());
    if (hit) return hit;
  }
  return fonts[0] ?? "";
}

export function ensureTextFontFamily(
  current: string,
  fonts: readonly string[],
): string {
  const cur = current.trim();
  if (cur && fonts.some((f) => f === cur)) return cur;
  if (cur) {
    const lower = cur.toLowerCase();
    const hit = fonts.find((f) => f.toLowerCase() === lower);
    if (hit) return hit;
  }
  return pickDefaultHeitiFont(fonts);
}
