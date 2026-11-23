/** Shared decode of the same comic URL across split halves (refcount + ImageBitmap). */

type CacheEntry = {
  promise: Promise<HTMLImageElement | ImageBitmap>;
  value: HTMLImageElement | ImageBitmap | null;
  refs: number;
};

const cache = new Map<string, CacheEntry>();

export function acquirePageSource(
  src: string,
  load: () => Promise<HTMLImageElement | ImageBitmap>,
): Promise<HTMLImageElement | ImageBitmap> {
  let entry = cache.get(src);
  if (!entry) {
    entry = {
      promise: load().then((value) => {
        const current = cache.get(src);
        if (current) current.value = value;
        return value;
      }),
      value: null,
      refs: 0,
    };
    cache.set(src, entry);
  }
  entry.refs += 1;
  return entry.promise;
}

export function releasePageSource(src: string) {
  const entry = cache.get(src);
  if (!entry) return;
  entry.refs -= 1;
  if (entry.refs > 0) return;
  cache.delete(src);
  const value = entry.value;
  if (value && "close" in value) {
    try {
      value.close();
    } catch {
      // already closed
    }
  }
}
