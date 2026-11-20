export type OcrRegion = {
  id: string;
  /** Normalized 0–1 relative to page width/height. */
  x: number;
  y: number;
  w: number;
  h: number;
  text: string;
  translation?: string;
  /** Ink / text color (#rrggbb). */
  fgColor?: string;
  /** Bubble/paper fill for masks (#rrggbb). */
  bgColor?: string;
  /** Glyph stroke / outline (#rrggbb), from 48px color head when available. */
  strokeColor?: string;
  /** Saber auto direction from textline long-edge vote. */
  vertical?: boolean;
};

export type OcrPageResult = {
  regions: OcrRegion[];
  cached: boolean;
  engine?: string;
  /** Primary text detector id (`off` / `ctd` / `default` / …). */
  textDetector?: string;
  auxYolo?: boolean;
  saberYoloRefine?: boolean;
  sourceLang?: string;
  targetLang?: string;
  translationProvider?: string;
  /** Page image path when available. */
  imagePath?: string;
  /** Stable `TRANSLATE_*` code when OCR ok but translation failed. */
  translationError?: string;
};
