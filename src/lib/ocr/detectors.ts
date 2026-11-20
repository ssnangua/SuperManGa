/** User-selectable primary text detectors (mutually exclusive). */
export type TextDetectorId = "off" | "ctd" | "dbnet" | "yolo" | "saber_yolo";

export type TextDetectorMeta = {
  id: TextDetectorId;
  /** i18n label key */
  nameKey: string;
  /** i18n hint key */
  hintKey: string;
  /** Asset pack id; null = always available (off). */
  assetId: string | null;
  requiresMerge: boolean;
};

export const TEXT_DETECTORS: readonly TextDetectorMeta[] = [
  {
    id: "off",
    nameKey: "textDetectorOff",
    hintKey: "textDetectorOffHint",
    assetId: null,
    requiresMerge: false,
  },
  {
    id: "dbnet",
    nameKey: "textDetectorDefault",
    hintKey: "textDetectorDefaultHint",
    assetId: "detector-dbnet",
    requiresMerge: true,
  },
  {
    id: "ctd",
    nameKey: "textDetectorCtd",
    hintKey: "textDetectorCtdHint",
    assetId: "detector-ctd",
    requiresMerge: true,
  },
  {
    id: "yolo",
    nameKey: "textDetectorYolo",
    hintKey: "textDetectorYoloHint",
    assetId: "detector-yolo",
    requiresMerge: true,
  },
  {
    id: "saber_yolo",
    nameKey: "textDetectorSaberYolo",
    hintKey: "textDetectorSaberYoloHint",
    assetId: "detector-saber-yolo",
    requiresMerge: false,
  },
] as const;

export const TEXT_DETECTOR_IDS: readonly TextDetectorId[] = TEXT_DETECTORS.map((d) => d.id);

export function normalizeTextDetectorId(value: unknown): TextDetectorId {
  if (value === "default") return "dbnet";
  if (typeof value === "string" && (TEXT_DETECTOR_IDS as readonly string[]).includes(value)) {
    return value as TextDetectorId;
  }
  return "dbnet";
}

export function textDetectorMeta(id: TextDetectorId): TextDetectorMeta {
  return TEXT_DETECTORS.find((d) => d.id === id) ?? TEXT_DETECTORS[0]!;
}

export function auxYoloAllowed(primary: TextDetectorId): boolean {
  return primary !== "off" && primary !== "yolo";
}

export function saberYoloRefineAllowed(primary: TextDetectorId): boolean {
  return primary !== "off" && primary !== "saber_yolo";
}
