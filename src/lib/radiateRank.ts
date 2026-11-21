/**
 * Background-fill order around a focus band `[lo, hi]` (inclusive).
 * Closer pages first; at the same distance, forward (`hi+d`) before backward (`lo-d`).
 * Example lo=hi=50 → ranks favor 51, 49, 52, 48, 53, 47, …
 */
export function radiateBatchRank(index: number, lo: number, hi: number): number {
  if (index > hi) return (index - hi) * 2;
  if (index < lo) return (lo - index) * 2 + 1;
  return 0;
}
