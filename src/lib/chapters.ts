import {
  compileEnabledChapterRules,
  matchChapterTitle,
  type ChapterFilenameRule,
} from "./chapterRules";

export interface PageRef {
  name: string;
  relPath: string;
}

export interface Chapter {
  page: number;
  title: string;
}

export function parentDir(relPath: string): string {
  const norm = relPath.replace(/\\/g, "/");
  const slash = norm.lastIndexOf("/");
  return slash === -1 ? "" : norm.slice(0, slash);
}

export function buildChapters(
  pages: PageRef[],
  rules: ChapterFilenameRule[],
): { chapters: Chapter[]; invalidRule: boolean } {
  // Multiple distinct parent dirs → each folder is a chapter.
  // A single wrapper folder (common zip layout) or a flat list → filename rules
  // on the basename, including images nested under that one folder.
  const uniqueDirs = new Set(pages.map((page) => parentDir(page.relPath)));
  if (uniqueDirs.size > 1) {
    const seen = new Set<string>();
    const chapters: Chapter[] = [];
    pages.forEach((page, index) => {
      const dir = parentDir(page.relPath);
      if (seen.has(dir)) return;
      seen.add(dir);
      chapters.push({ page: index + 1, title: dir || "" });
    });
    return { chapters, invalidRule: false };
  }

  const compiled = compileEnabledChapterRules(rules);
  if (!compiled.ok) {
    return { chapters: [], invalidRule: compiled.error === "invalid" };
  }

  const seen = new Set<string>();
  const chapters: Chapter[] = [];
  pages.forEach((page, index) => {
    const title = matchChapterTitle(page.name, rules);
    if (!title || seen.has(title)) return;
    seen.add(title);
    chapters.push({ page: index + 1, title });
  });
  return { chapters, invalidRule: false };
}

/** Index of the chapter covering `page` (1-based), or -1. */
export function chapterIndexAtPage(chapters: Chapter[], page: number): number {
  let idx = -1;
  for (let i = 0; i < chapters.length; i++) {
    const chapter = chapters[i];
    if (!chapter) continue;
    if (chapter.page <= page) idx = i;
    else break;
  }
  return idx;
}

/** Page count of chapter at `index` (images in that chapter). `totalPages` is 1-based book length. */
export function chapterPageCount(
  chapters: Chapter[],
  index: number,
  totalPages: number,
): number {
  const chapter = chapters[index];
  if (!chapter || totalPages <= 0) return 0;
  const next = chapters[index + 1];
  const end = next ? next.page - 1 : totalPages;
  return Math.max(0, end - chapter.page + 1);
}
