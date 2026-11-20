export interface Bookmark {
  page: number;
  comment: string;
}

export function parseBookmarks(text: string): Bookmark[] {
  const bookmarks: Bookmark[] = [];
  for (const line of text.split(/\r?\n/)) {
    const match = line.trim().match(/^(\d+):(.*)$/);
    if (!match) continue;
    const page = Number.parseInt(match[1], 10);
    if (!Number.isFinite(page)) continue;
    bookmarks.push({ page, comment: match[2] });
  }
  return bookmarks;
}

export function serializeBookmarks(bookmarks: Bookmark[]): string {
  return [...bookmarks]
    .sort((a, b) => a.page - b.page || a.comment.localeCompare(b.comment))
    .map((bookmark) => `${bookmark.page}:${bookmark.comment}`)
    .join("\n");
}
