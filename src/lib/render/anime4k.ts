import {
  ANIME4KJS_SIMPLE_M_2X,
  ANIME4KJS_SIMPLE_S_2X,
  Anime4K_Clamp_Highlights,
  Anime4K_Restore_CNN_M,
  Anime4K_Restore_CNN_S,
  ImageUpscaler,
  type Anime4KShaderConstructor,
} from "anime4k.js";
import { maxTextureSize, sourcePixelSize } from "./textureLimit";

export type Anime4kStrength = "anime4kWeak" | "anime4kStrong";

/** Soft cap per tile — CNN float passes need headroom below MAX_TEXTURE_SIZE. */
const ANIME4K_TILE_EDGE = 2048;
const TILE_OVERLAP = 64;

const RESTORE_ONLY_S: Anime4KShaderConstructor[] = [
  Anime4K_Clamp_Highlights,
  Anime4K_Restore_CNN_S,
];

const RESTORE_ONLY_M: Anime4KShaderConstructor[] = [
  Anime4K_Clamp_Highlights,
  Anime4K_Restore_CNN_M,
];

function needsUpscale(srcW: number, srcH: number, dstW: number, dstH: number) {
  return dstW > srcW * 1.2 || dstH > srcH * 1.2;
}

function presetFor(strength: Anime4kStrength, upscale: boolean): Anime4KShaderConstructor[] {
  if (strength === "anime4kWeak") {
    return upscale ? ANIME4KJS_SIMPLE_S_2X : RESTORE_ONLY_S;
  }
  return upscale ? ANIME4KJS_SIMPLE_M_2X : RESTORE_ONLY_M;
}

function createCanvas(width: number, height: number) {
  const canvas = document.createElement("canvas");
  canvas.width = Math.max(1, width);
  canvas.height = Math.max(1, height);
  return canvas;
}

function tileEdge(scaleFactor: number) {
  const tex = Math.max(256, Math.floor(maxTextureSize() / Math.max(1, scaleFactor)));
  return Math.max(256, Math.min(ANIME4K_TILE_EDGE, tex));
}

type DestMap = {
  scaleX: number;
  scaleY: number;
  offsetX: number;
  offsetY: number;
};

function destMap(
  srcW: number,
  srcH: number,
  displayW: number,
  displayH: number,
  objectFit: "fill" | "contain-top",
): DestMap {
  if (objectFit === "contain-top") {
    const scale = Math.min(displayW / Math.max(1, srcW), displayH / Math.max(1, srcH));
    return {
      scaleX: scale,
      scaleY: scale,
      offsetX: (displayW - srcW * scale) / 2,
      offsetY: 0,
    };
  }
  return {
    scaleX: displayW / Math.max(1, srcW),
    scaleY: displayH / Math.max(1, srcH),
    offsetX: 0,
    offsetY: 0,
  };
}

export class Anime4kPresenter {
  private strength: Anime4kStrength;
  private upscaler: ImageUpscaler | null = null;
  private tileIn: HTMLCanvasElement | null = null;
  private tileOut: HTMLCanvasElement | null = null;
  private presetKey = "";

  constructor(strength: Anime4kStrength) {
    this.strength = strength;
  }

  get currentProfile() {
    return this.strength;
  }

  static isSupported() {
    try {
      return ImageUpscaler.isSupported();
    } catch {
      return false;
    }
  }

  draw(
    image: CanvasImageSource,
    canvas: HTMLCanvasElement,
    displayW: number,
    displayH: number,
    objectFit: "fill" | "contain-top" = "fill",
    dpr = 1,
  ) {
    if (!Anime4kPresenter.isSupported()) {
      throw new Error("Anime4K WebGL extensions unavailable");
    }
    const natural = sourcePixelSize(image);
    const cssW = displayW / Math.max(0.01, dpr);
    const cssH = displayH / Math.max(0.01, dpr);
    const willUpscale = needsUpscale(natural.width, natural.height, cssW, cssH);
    const mag = willUpscale ? 2 : 1;
    const preset = presetFor(this.strength, willUpscale);
    const edge = tileEdge(mag);
    const map = destMap(natural.width, natural.height, displayW, displayH, objectFit);

    if (canvas.width !== displayW || canvas.height !== displayH) {
      canvas.width = displayW;
      canvas.height = displayH;
    }
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.clearRect(0, 0, displayW, displayH);

    const needsTiles =
      natural.width > edge || natural.height > edge;

    try {
      if (!needsTiles) {
        this.runSingle(image, natural.width, natural.height, preset, mag, ctx, map);
        return;
      }
      this.runTiled(image, natural.width, natural.height, preset, mag, edge, ctx, map);
    } catch {
      this.disposePipeline();
      // Fallback: plain blit keeps the page readable if GPU chokes mid-tile.
      ctx.drawImage(
        image,
        0,
        0,
        natural.width,
        natural.height,
        map.offsetX,
        map.offsetY,
        natural.width * map.scaleX,
        natural.height * map.scaleY,
      );
    }
  }

  private ensurePipeline(
    preset: Anime4KShaderConstructor[],
    tileW: number,
    tileH: number,
    mag: number,
    key: string,
  ) {
    if (
      this.upscaler &&
      this.presetKey === key &&
      this.tileIn &&
      this.tileOut &&
      this.tileIn.width === tileW &&
      this.tileIn.height === tileH
    ) {
      return;
    }
    this.disposePipeline();
    this.upscaler = new ImageUpscaler(preset);
    this.tileIn = createCanvas(tileW, tileH);
    this.tileOut = createCanvas(tileW * mag, tileH * mag);
    this.upscaler.attachSource(this.tileIn, this.tileOut);
    this.presetKey = key;
  }

  private runSingle(
    image: CanvasImageSource,
    srcW: number,
    srcH: number,
    preset: Anime4KShaderConstructor[],
    mag: number,
    ctx: CanvasRenderingContext2D,
    map: DestMap,
  ) {
    const key = `${this.strength}:single:${mag}:${srcW}x${srcH}`;
    this.ensurePipeline(preset, srcW, srcH, mag, key);
    const input = this.tileIn!;
    const output = this.tileOut!;
    const inputCtx = input.getContext("2d");
    if (!inputCtx || !this.upscaler) return;
    inputCtx.clearRect(0, 0, srcW, srcH);
    inputCtx.drawImage(image, 0, 0, srcW, srcH, 0, 0, srcW, srcH);
    this.upscaler.upscale();
    ctx.drawImage(
      output,
      0,
      0,
      output.width,
      output.height,
      map.offsetX,
      map.offsetY,
      srcW * map.scaleX,
      srcH * map.scaleY,
    );
  }

  private runTiled(
    image: CanvasImageSource,
    srcW: number,
    srcH: number,
    preset: Anime4KShaderConstructor[],
    mag: number,
    edge: number,
    ctx: CanvasRenderingContext2D,
    map: DestMap,
  ) {
    const overlap = Math.min(TILE_OVERLAP, Math.floor(edge / 4));
    const stepX = Math.max(1, edge - overlap);
    const stepY = Math.max(1, edge - overlap);
    // Fixed tile canvas size so the GL pipeline can be reused across strips.
    const tileW = Math.min(edge, srcW);
    const tileH = Math.min(edge, srcH);
    const key = `${this.strength}:tile:${mag}:${tileW}x${tileH}`;
    this.ensurePipeline(preset, tileW, tileH, mag, key);
    const input = this.tileIn!;
    const output = this.tileOut!;
    const inputCtx = input.getContext("2d");
    if (!inputCtx || !this.upscaler) return;

    for (let originY = 0; originY < srcH; originY += stepY) {
      for (let originX = 0; originX < srcW; originX += stepX) {
        const readW = Math.min(tileW, srcW - originX);
        const readH = Math.min(tileH, srcH - originY);
        inputCtx.clearRect(0, 0, tileW, tileH);
        // Pad short edge tiles by clamping source samples (drawImage with smaller src).
        inputCtx.drawImage(
          image,
          originX,
          originY,
          readW,
          readH,
          0,
          0,
          readW,
          readH,
        );
        // Extend pad with edge pixels so CNN border sees something stable.
        if (readW < tileW) {
          inputCtx.drawImage(input, readW - 1, 0, 1, readH, readW, 0, tileW - readW, readH);
        }
        if (readH < tileH) {
          inputCtx.drawImage(input, 0, readH - 1, tileW, 1, 0, readH, tileW, tileH - readH);
        }

        this.upscaler.upscale();

        const lastX = originX + stepX >= srcW;
        const lastY = originY + stepY >= srcH;
        const insetLeft = originX === 0 ? 0 : Math.floor(overlap / 2);
        const insetTop = originY === 0 ? 0 : Math.floor(overlap / 2);
        const insetRight = lastX ? 0 : Math.ceil(overlap / 2);
        const insetBottom = lastY ? 0 : Math.ceil(overlap / 2);

        const contentX = insetLeft;
        const contentY = insetTop;
        const contentW = Math.max(1, readW - insetLeft - insetRight);
        const contentH = Math.max(1, readH - insetTop - insetBottom);

        const outX = contentX * mag;
        const outY = contentY * mag;
        const outW = contentW * mag;
        const outH = contentH * mag;

        const destX = map.offsetX + (originX + contentX) * map.scaleX;
        const destY = map.offsetY + (originY + contentY) * map.scaleY;
        const destW = contentW * map.scaleX;
        const destH = contentH * map.scaleY;

        ctx.drawImage(output, outX, outY, outW, outH, destX, destY, destW, destH);
      }
    }
  }

  private disposePipeline() {
    this.upscaler?.detachSource();
    this.upscaler = null;
    this.tileIn = null;
    this.tileOut = null;
    this.presetKey = "";
  }

  dispose() {
    this.disposePipeline();
  }
}
