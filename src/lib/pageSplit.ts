export type ReadingDirection = "ltr" | "rtl";
export type PageHalf = "full" | "left" | "right";

export function logicalPageCount(physicalCount: number, split: boolean): number {
  if (physicalCount <= 0) return 0;
  return split ? physicalCount * 2 : physicalCount;
}

export function physicalPageIndex(logical: number, split: boolean): number {
  if (!split) return Math.max(0, logical);
  return Math.max(0, Math.floor(logical / 2));
}

/** RTL (右开): even = right half first; LTR: even = left half first. */
export function pageHalf(
  logical: number,
  split: boolean,
  direction: ReadingDirection,
): PageHalf {
  if (!split) return "full";
  const second = logical % 2 === 1;
  if (direction === "rtl") return second ? "left" : "right";
  return second ? "right" : "left";
}

/**
 * Map a physical page (+ optional half) back to a logical index.
 * With split on and half `full` (e.g. history restore), lands on the first half of that page.
 */
export function logicalFromPhysical(
  physical: number,
  half: PageHalf,
  split: boolean,
  direction: ReadingDirection,
): number {
  if (!split) return Math.max(0, physical);
  const base = Math.max(0, Math.floor(physical)) * 2;
  if (half === "full") return base;
  const leftIsSecond = direction === "rtl";
  if (half === "left") return base + (leftIsSecond ? 1 : 0);
  return base + (leftIsSecond ? 0 : 1);
}

/** Remap current logical index when toggling split on/off. */
export function remapLogicalIndex(
  logical: number,
  fromSplit: boolean,
  toSplit: boolean,
  direction: ReadingDirection,
): number {
  if (fromSplit === toSplit) return logical;
  if (!fromSplit && toSplit) {
    return logicalFromPhysical(logical, "full", true, direction);
  }
  return physicalPageIndex(logical, true);
}

/** Remap when reading direction flips while split is on (keep same physical + half). */
export function remapLogicalIndexForDirection(
  logical: number,
  split: boolean,
  fromDir: ReadingDirection,
  toDir: ReadingDirection,
): number {
  if (!split || fromDir === toDir) return logical;
  const physical = physicalPageIndex(logical, true);
  const half = pageHalf(logical, true, fromDir);
  return logicalFromPhysical(physical, half, true, toDir);
}

/** Both logical indexes that belong to one physical page when split. */
export function logicalPairForPhysical(physical: number, split: boolean): number[] {
  if (!split) return [physical];
  return [physical * 2, physical * 2 + 1];
}

/** Clip a normalized OCR box into a half-page local 0–1 frame. */
export function clipRegionToHalf(
  region: { x: number; y: number; w: number; h: number },
  half: PageHalf,
): { x: number; y: number; w: number; h: number } | null {
  if (half === "full") return { ...region };
  const mid = 0.5;
  const left = region.x;
  const right = region.x + region.w;
  if (half === "left") {
    if (right <= 0 || left >= mid) return null;
    const x0 = Math.max(0, left);
    const x1 = Math.min(mid, right);
    if (x1 <= x0) return null;
    // Prefer side with larger overlap when straddling mid.
    if (left < mid && right > mid) {
      const leftArea = (mid - left) * region.h;
      const rightArea = (right - mid) * region.h;
      if (rightArea > leftArea) return null;
    }
    return { x: x0 / mid, y: region.y, w: (x1 - x0) / mid, h: region.h };
  }
  // right half
  if (right <= mid || left >= 1) return null;
  const x0 = Math.max(mid, left);
  const x1 = Math.min(1, right);
  if (x1 <= x0) return null;
  if (left < mid && right > mid) {
    const leftArea = (mid - left) * region.h;
    const rightArea = (right - mid) * region.h;
    if (leftArea >= rightArea) return null;
  }
  return { x: (x0 - mid) / mid, y: region.y, w: (x1 - x0) / mid, h: region.h };
}
