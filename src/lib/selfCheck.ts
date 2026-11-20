import { buildChapters } from "./chapters";
import { createDefaultChapterRules } from "./chapterRules";
import { parseBookmarks, serializeBookmarks } from "./bookmarks";
import { spreadIndexes, spreadStart } from "./spread";

export function selfCheck() {
  const rules = createDefaultChapterRules();
  const flat = buildChapters(
    [
      { name: "1_01.jpg", relPath: "1_01.jpg" },
      { name: "1_02.jpg", relPath: "1_02.jpg" },
      { name: "2_01.jpg", relPath: "2_01.jpg" },
    ],
    rules,
  );
  if (flat.chapters.length !== 2 || flat.chapters[0]?.title !== "1" || flat.chapters[1]?.page !== 3) {
    throw new Error("chapter regex check failed");
  }
  const dirs = buildChapters(
    [
      { name: "a.jpg", relPath: "c1/a.jpg" },
      { name: "b.jpg", relPath: "c2/b.jpg" },
    ],
    rules,
  );
  if (dirs.chapters.length !== 2 || dirs.chapters[0]?.title !== "c1") {
    throw new Error("chapter folder check failed");
  }
  const wrapped = buildChapters(
    [
      { name: "1_01.jpg", relPath: "book/1_01.jpg" },
      { name: "1_02.jpg", relPath: "book/1_02.jpg" },
      { name: "2_01.jpg", relPath: "book/2_01.jpg" },
    ],
    rules,
  );
  if (
    wrapped.chapters.length !== 2
    || wrapped.chapters[0]?.title !== "1"
    || wrapped.chapters[1]?.page !== 3
  ) {
    throw new Error("chapter single-folder wrapper check failed");
  }
  const bookmarks = parseBookmarks("12:高潮\n3:开头\nbad");
  if (bookmarks.length !== 2 || serializeBookmarks(bookmarks) !== "3:开头\n12:高潮") {
    throw new Error("bookmark format check failed");
  }
  if (spreadStart(4, 10, "double", true) !== 3) {
    throw new Error("spread check failed");
  }
  if (spreadIndexes(0, 10, "double", true).join() !== "0") {
    throw new Error("cover spread check failed");
  }
}
