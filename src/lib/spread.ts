export type ReadingMode = "scroll" | "single" | "double";

export function spreadStart(
  index: number,
  count: number,
  mode: ReadingMode,
  isolateCover: boolean,
): number {
  if (count <= 0) return 0;
  const clamped = Math.max(0, Math.min(count - 1, index));
  if (mode !== "double") return clamped;
  if (isolateCover) {
    if (clamped === 0) return 0;
    return 1 + Math.floor((clamped - 1) / 2) * 2;
  }
  return Math.floor(clamped / 2) * 2;
}

export function spreadIndexes(
  index: number,
  count: number,
  mode: ReadingMode,
  isolateCover: boolean,
): number[] {
  const start = spreadStart(index, count, mode, isolateCover);
  if (mode !== "double" || (isolateCover && start === 0)) return [start];
  return start + 1 < count ? [start, start + 1] : [start];
}
