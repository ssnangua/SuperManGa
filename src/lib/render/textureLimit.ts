/** WebGL / canvas GPU texture limits for huge webtoon pages. */

let cachedMax = 0;

export function maxTextureSize(): number {
  if (cachedMax > 0) return cachedMax;
  let size = 4096;
  try {
    const canvas = document.createElement("canvas");
    const gl =
      canvas.getContext("webgl", { failIfMajorPerformanceCaveat: false }) ||
      canvas.getContext("experimental-webgl");
    if (gl && "getParameter" in gl) {
      const reported = Number(
        (gl as WebGLRenderingContext).getParameter(
          (gl as WebGLRenderingContext).MAX_TEXTURE_SIZE,
        ),
      );
      if (Number.isFinite(reported) && reported > 0) size = reported;
      const lose = (gl as WebGLRenderingContext).getExtension("WEBGL_lose_context");
      lose?.loseContext();
    }
  } catch {
    // keep fallback
  }
  // Stay slightly under the limit — some drivers fail near the edge.
  cachedMax = Math.max(2048, Math.floor(size * 0.95));
  return cachedMax;
}

export function sourcePixelSize(image: CanvasImageSource) {
  if (typeof ImageBitmap !== "undefined" && image instanceof ImageBitmap) {
    return { width: image.width, height: image.height };
  }
  if (typeof HTMLImageElement !== "undefined" && image instanceof HTMLImageElement) {
    return { width: image.naturalWidth, height: image.naturalHeight };
  }
  if (typeof HTMLCanvasElement !== "undefined" && image instanceof HTMLCanvasElement) {
    return { width: image.width, height: image.height };
  }
  if (typeof OffscreenCanvas !== "undefined" && image instanceof OffscreenCanvas) {
    return { width: image.width, height: image.height };
  }
  const anyImage = image as { width?: number; height?: number };
  return { width: anyImage.width || 1, height: anyImage.height || 1 };
}

