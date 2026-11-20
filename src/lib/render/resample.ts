export type ResampleKind =
  | "average"
  | "bicubic"
  | "lanczos"
  | "averageSharpWeak"
  | "averageSharpStrong";

function clamp(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value));
}

function clampByte(value: number) {
  return value < 0 ? 0 : value > 255 ? 255 : value;
}

type Tap = { index: number; weight: number };

function lanczosWeight(x: number, a: number) {
  const ax = Math.abs(x);
  if (ax < 1e-8) return 1;
  if (ax >= a) return 0;
  const px = Math.PI * x;
  return (a * Math.sin(px) * Math.sin(px / a)) / (px * px);
}

function cubicWeight(t: number) {
  const at = Math.abs(t);
  if (at <= 1) return 1.5 * at * at * at - 2.5 * at * at + 1;
  if (at < 2) return -0.5 * at * at * at + 2.5 * at * at - 4 * at + 2;
  return 0;
}

function buildKernel(
  destSize: number,
  srcSize: number,
  radius: number,
  weightAt: (t: number) => number,
): Tap[][] {
  const scale = srcSize / destSize;
  const kernels: Tap[][] = new Array(destSize);
  for (let i = 0; i < destSize; i += 1) {
    const center = (i + 0.5) * scale - 0.5;
    const left = Math.floor(center - radius + 1e-6);
    const right = Math.ceil(center + radius - 1e-6);
    const taps: Tap[] = [];
    let sum = 0;
    for (let s = left; s <= right; s += 1) {
      const weight = weightAt(center - s);
      if (weight === 0) continue;
      const index = clamp(s, 0, srcSize - 1);
      taps.push({ index, weight });
      sum += weight;
    }
    if (sum !== 0 && sum !== 1) {
      for (const tap of taps) tap.weight /= sum;
    }
    if (taps.length === 0) {
      taps.push({ index: clamp(Math.round(center), 0, srcSize - 1), weight: 1 });
    }
    kernels[i] = taps;
  }
  return kernels;
}

function separableResample(
  source: ImageData,
  targetWidth: number,
  targetHeight: number,
  radius: number,
  weightAt: (t: number) => number,
): ImageData {
  const tw = Math.max(1, Math.round(targetWidth));
  const th = Math.max(1, Math.round(targetHeight));
  const { width, height, data } = source;
  if (width === tw && height === th) return source;

  const xKernel = buildKernel(tw, width, radius, weightAt);
  const yKernel = buildKernel(th, height, radius, weightAt);
  const temp = new Float32Array(tw * height * 4);

  for (let y = 0; y < height; y += 1) {
    const row = y * width * 4;
    for (let x = 0; x < tw; x += 1) {
      const taps = xKernel[x]!;
      let r = 0;
      let g = 0;
      let b = 0;
      let a = 0;
      for (let t = 0; t < taps.length; t += 1) {
        const { index, weight } = taps[t]!;
        const i = row + index * 4;
        r += data[i]! * weight;
        g += data[i + 1]! * weight;
        b += data[i + 2]! * weight;
        a += data[i + 3]! * weight;
      }
      const o = (y * tw + x) * 4;
      temp[o] = r;
      temp[o + 1] = g;
      temp[o + 2] = b;
      temp[o + 3] = a;
    }
  }

  const out = new ImageData(tw, th);
  const dst = out.data;
  for (let y = 0; y < th; y += 1) {
    const taps = yKernel[y]!;
    for (let x = 0; x < tw; x += 1) {
      let r = 0;
      let g = 0;
      let b = 0;
      let a = 0;
      for (let t = 0; t < taps.length; t += 1) {
        const { index, weight } = taps[t]!;
        const i = (index * tw + x) * 4;
        r += temp[i]! * weight;
        g += temp[i + 1]! * weight;
        b += temp[i + 2]! * weight;
        a += temp[i + 3]! * weight;
      }
      const o = (y * tw + x) * 4;
      dst[o] = clampByte(r);
      dst[o + 1] = clampByte(g);
      dst[o + 2] = clampByte(b);
      dst[o + 3] = clampByte(a);
    }
  }
  return out;
}

function averageResample(source: ImageData, targetWidth: number, targetHeight: number): ImageData {
  const tw = Math.max(1, Math.round(targetWidth));
  const th = Math.max(1, Math.round(targetHeight));
  const { width, height, data } = source;
  if (width === tw && height === th) return source;

  // Upscale: area-average is undefined/soft — use sharp separable cubic instead.
  if (tw >= width && th >= height) {
    return separableResample(source, tw, th, 2, cubicWeight);
  }

  const out = new ImageData(tw, th);
  const dst = out.data;
  const scaleX = width / tw;
  const scaleY = height / th;
  for (let y = 0; y < th; y += 1) {
    const y0 = y * scaleY;
    const y1 = (y + 1) * scaleY;
    const top = Math.floor(y0);
    const bottom = Math.min(height, Math.ceil(y1));
    for (let x = 0; x < tw; x += 1) {
      const x0 = x * scaleX;
      const x1 = (x + 1) * scaleX;
      const left = Math.floor(x0);
      const right = Math.min(width, Math.ceil(x1));
      let r = 0;
      let g = 0;
      let b = 0;
      let a = 0;
      let area = 0;
      for (let sy = top; sy < bottom; sy += 1) {
        const rowTop = Math.max(y0, sy);
        const rowBottom = Math.min(y1, sy + 1);
        const yCover = rowBottom - rowTop;
        if (yCover <= 0) continue;
        for (let sx = left; sx < right; sx += 1) {
          const colLeft = Math.max(x0, sx);
          const colRight = Math.min(x1, sx + 1);
          const cover = (colRight - colLeft) * yCover;
          if (cover <= 0) continue;
          const i = (sy * width + sx) * 4;
          r += data[i]! * cover;
          g += data[i + 1]! * cover;
          b += data[i + 2]! * cover;
          a += data[i + 3]! * cover;
          area += cover;
        }
      }
      const o = (y * tw + x) * 4;
      if (area > 0) {
        dst[o] = clampByte(r / area);
        dst[o + 1] = clampByte(g / area);
        dst[o + 2] = clampByte(b / area);
        dst[o + 3] = clampByte(a / area);
      } else {
        dst[o + 3] = 255;
      }
    }
  }
  return out;
}

export function sharpenImageData(source: ImageData, amount: number): ImageData {
  const { width, height, data } = source;
  const out = new ImageData(width, height);
  const dst = out.data;
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      const i = (y * width + x) * 4;
      for (let c = 0; c < 3; c += 1) {
        const c0 = data[i + c] ?? 0;
        const n = data[((y > 0 ? y - 1 : y) * width + x) * 4 + c] ?? c0;
        const s = data[((y + 1 < height ? y + 1 : y) * width + x) * 4 + c] ?? c0;
        const w = data[(y * width + (x > 0 ? x - 1 : x)) * 4 + c] ?? c0;
        const e = data[(y * width + (x + 1 < width ? x + 1 : x)) * 4 + c] ?? c0;
        dst[i + c] = clampByte(c0 * (1 + 4 * amount) - (n + s + e + w) * amount);
      }
      dst[i + 3] = data[i + 3] ?? 255;
    }
  }
  return out;
}

export function resampleImageData(
  source: ImageData,
  targetWidth: number,
  targetHeight: number,
  kind: ResampleKind,
): ImageData {
  const tw = Math.max(1, Math.round(targetWidth));
  const th = Math.max(1, Math.round(targetHeight));

  if (kind === "average" || kind === "averageSharpWeak" || kind === "averageSharpStrong") {
    const averaged = averageResample(source, tw, th);
    if (kind === "averageSharpWeak") return sharpenImageData(averaged, 0.35);
    if (kind === "averageSharpStrong") return sharpenImageData(averaged, 0.7);
    return averaged;
  }

  if (kind === "lanczos") {
    // Separable Lanczos-3: much faster than 2D Lanczos, keeps sharp line art.
    return separableResample(source, tw, th, 3, (t) => lanczosWeight(t, 3));
  }

  return separableResample(source, tw, th, 2, cubicWeight);
}

export async function imageToImageData(
  image: CanvasImageSource,
  width: number,
  height: number,
): Promise<ImageData> {
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const ctx = canvas.getContext("2d", { willReadFrequently: true });
  if (!ctx) throw new Error("2d context unavailable");
  ctx.drawImage(image, 0, 0, width, height);
  return ctx.getImageData(0, 0, width, height);
}
