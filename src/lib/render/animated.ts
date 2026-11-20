function u32le(bytes: Uint8Array, offset: number): number {
  return (
    bytes[offset]! |
    (bytes[offset + 1]! << 8) |
    (bytes[offset + 2]! << 16) |
    (bytes[offset + 3]! << 24)
  ) >>> 0;
}

function ascii(bytes: Uint8Array, offset: number, length: number): string {
  return String.fromCharCode(...bytes.subarray(offset, offset + length));
}

function skipGifSubBlocks(bytes: Uint8Array, offset: number): number {
  let i = offset;
  while (i < bytes.length) {
    const size = bytes[i]!;
    i += 1;
    if (size === 0) return i;
    i += size;
  }
  return i;
}

function isAnimatedGif(bytes: Uint8Array): boolean {
  if (bytes.length < 13) return false;
  if (ascii(bytes, 0, 3) !== "GIF") return false;
  let i = 13;
  const packed = bytes[10]!;
  if (packed & 0x80) {
    i += 3 * (1 << ((packed & 7) + 1));
  }
  let frames = 0;
  while (i < bytes.length) {
    const marker = bytes[i]!;
    if (marker === 0x3b) break;
    if (marker === 0x2c) {
      frames += 1;
      if (frames >= 2) return true;
      if (i + 10 >= bytes.length) break;
      const imgPacked = bytes[i + 9]!;
      i += 10;
      if (imgPacked & 0x80) {
        i += 3 * (1 << ((imgPacked & 7) + 1));
      }
      i += 1;
      i = skipGifSubBlocks(bytes, i);
      continue;
    }
    if (marker === 0x21) {
      i += 2;
      i = skipGifSubBlocks(bytes, i);
      continue;
    }
    i += 1;
  }
  return false;
}

function isAnimatedWebp(bytes: Uint8Array): boolean {
  if (bytes.length < 16) return false;
  if (ascii(bytes, 0, 4) !== "RIFF" || ascii(bytes, 8, 4) !== "WEBP") return false;
  let i = 12;
  while (i + 8 <= bytes.length) {
    const fourcc = ascii(bytes, i, 4);
    const size = u32le(bytes, i + 4);
    const data = i + 8;
    if (fourcc === "ANIM") return true;
    if (fourcc === "VP8X" && data < bytes.length && (bytes[data]! & 0x02) !== 0) return true;
    i = data + size + (size % 2);
  }
  return false;
}

function isAnimatedPng(bytes: Uint8Array): boolean {
  if (bytes.length < 24) return false;
  const png = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
  for (let n = 0; n < 8; n += 1) {
    if (bytes[n] !== png[n]) return false;
  }
  let i = 8;
  while (i + 12 <= bytes.length) {
    const size = (bytes[i]! << 24) | (bytes[i + 1]! << 16) | (bytes[i + 2]! << 8) | bytes[i + 3]!;
    const type = ascii(bytes, i + 4, 4);
    if (type === "acTL") return true;
    if (type === "IDAT" || type === "IEND") return false;
    i += 12 + size;
  }
  return false;
}

/** True when GIF / animated WebP / APNG has more than one frame. */
export function isAnimatedImage(bytes: Uint8Array): boolean {
  if (bytes.length < 12) return false;
  if (bytes[0] === 0x47 && bytes[1] === 0x49 && bytes[2] === 0x46) return isAnimatedGif(bytes);
  if (bytes[0] === 0x52 && bytes[1] === 0x49 && bytes[2] === 0x46 && bytes[3] === 0x46) {
    return isAnimatedWebp(bytes);
  }
  if (bytes[0] === 0x89 && bytes[1] === 0x50) return isAnimatedPng(bytes);
  return false;
}

export const ANIMATED_IMAGE_SKIP = "animated-image";

export function isAnimatedImageSkip(error: unknown): boolean {
  const message = error instanceof Error ? error.message : String(error ?? "");
  return message === ANIMATED_IMAGE_SKIP;
}
