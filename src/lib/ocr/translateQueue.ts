import { radiateBatchRank } from "../radiateRank";
import { spreadIndexes, type ReadingMode } from "../spread";

/** Interactive view (current spread / focus). */
export const TRANSLATE_PRIORITY_CURRENT = 0;
/** Prefetch next spread after current. */
export const TRANSLATE_PRIORITY_NEXT = 1;
/** Prefetch previous spread after next. */
export const TRANSLATE_PRIORITY_PREV = 2;
/** Full-book background fill. */
export const TRANSLATE_PRIORITY_BATCH = 3;

function focusSpread(
  focus: number,
  count: number,
  mode: ReadingMode,
  isolateCover: boolean,
): number[] {
  if (count <= 0) return [];
  if (mode !== "double") {
    const clamped = Math.max(0, Math.min(count - 1, focus));
    return [clamped];
  }
  return spreadIndexes(focus, count, "double", isolateCover);
}

function neighborSpread(
  from: number,
  count: number,
  mode: ReadingMode,
  isolateCover: boolean,
): number[] {
  if (count <= 0 || from < 0 || from >= count) return [];
  return mode === "double" ? spreadIndexes(from, count, "double", isolateCover) : [from];
}

export function translatePriorityForIndex(
  index: number,
  focus: number,
  count: number,
  mode: ReadingMode,
  isolateCover: boolean,
): number {
  if (count <= 0) return TRANSLATE_PRIORITY_BATCH;
  const current = focusSpread(focus, count, mode, isolateCover);
  if (current.includes(index)) return TRANSLATE_PRIORITY_CURRENT;
  const first = current[0] ?? focus;
  const last = current[current.length - 1] ?? focus;
  if (
    last + 1 < count &&
    neighborSpread(last + 1, count, mode, isolateCover).includes(index)
  ) {
    return TRANSLATE_PRIORITY_NEXT;
  }
  if (first > 0 && neighborSpread(first - 1, count, mode, isolateCover).includes(index)) {
    return TRANSLATE_PRIORITY_PREV;
  }
  return TRANSLATE_PRIORITY_BATCH;
}

/** Current spread + next/prev neighbor spreads (book pre-translate off still processes these). */
export function nearFocusTranslatePages(
  focus: number,
  count: number,
  mode: ReadingMode,
  isolateCover: boolean,
): number[] {
  if (count <= 0) return [];
  const set = new Set<number>();
  const current = focusSpread(focus, count, mode, isolateCover);
  for (const page of current) set.add(page);
  const first = current[0] ?? focus;
  const last = current[current.length - 1] ?? focus;
  if (last + 1 < count) {
    for (const page of neighborSpread(last + 1, count, mode, isolateCover)) set.add(page);
  }
  if (first > 0) {
    for (const page of neighborSpread(first - 1, count, mode, isolateCover)) set.add(page);
  }
  return [...set].sort((a, b) => a - b);
}

function batchRank(
  index: number,
  focus: number,
  count: number,
  mode: ReadingMode,
  isolateCover: boolean,
): number {
  const current = focusSpread(focus, count, mode, isolateCover);
  const lo = current[0] ?? focus;
  const hi = current[current.length - 1] ?? focus;
  return radiateBatchRank(index, lo, hi);
}

/** Sort pending page indexes: current → next → prev → batch (radiate from focus). */
export function rankTranslatePages(
  pages: number[],
  focus: number,
  count: number,
  mode: ReadingMode,
  isolateCover: boolean,
): number[] {
  return pages.slice().sort((a, b) => {
    const pa = translatePriorityForIndex(a, focus, count, mode, isolateCover);
    const pb = translatePriorityForIndex(b, focus, count, mode, isolateCover);
    if (pa !== pb) return pa - pb;
    if (pa >= TRANSLATE_PRIORITY_BATCH) {
      return (
        batchRank(a, focus, count, mode, isolateCover) -
        batchRank(b, focus, count, mode, isolateCover)
      );
    }
    return a - b;
  });
}
