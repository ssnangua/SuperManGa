export const CLASSIC_PROFILES = [
  "nearest",
  "bilinear",
  "average",
  "bicubic",
  "lanczos",
  "averageSharpWeak",
  "averageSharpStrong",
] as const;

export const REALTIME_PROFILES = [
  "anime4kWeak",
  "anime4kStrong",
] as const;

export type ClassicProfile = (typeof CLASSIC_PROFILES)[number];
export type RealtimeProfile = (typeof REALTIME_PROFILES)[number];
export type Anime4kProfile = "anime4kWeak" | "anime4kStrong";
export type NeuralProfile = `neural:${string}`;
export type RenderProfile = ClassicProfile | RealtimeProfile | NeuralProfile;

export function isClassicProfile(value: string): value is ClassicProfile {
  return (CLASSIC_PROFILES as readonly string[]).includes(value);
}

export function isRealtimeProfile(value: string): value is RealtimeProfile {
  return (REALTIME_PROFILES as readonly string[]).includes(value);
}

export function isAnime4kProfile(value: string): value is Anime4kProfile {
  return value === "anime4kWeak" || value === "anime4kStrong";
}

export function isNeuralProfile(value: string): value is NeuralProfile {
  return value.startsWith("neural:");
}

export function neuralAssetId(profile: NeuralProfile): string {
  return profile.slice("neural:".length);
}

export function neuralProfileForAsset(id: string): NeuralProfile {
  return `neural:${id}`;
}

export function normalizeRenderProfile(value: unknown): RenderProfile {
  if (typeof value !== "string" || !value) return "bicubic";
  if (isClassicProfile(value) || isRealtimeProfile(value) || isNeuralProfile(value)) return value;
  return "bicubic";
}

/** Modes that can stay on <img> + CSS. */
export function usesCssImage(profile: RenderProfile): boolean {
  return profile === "bilinear" || profile === "nearest" || isNeuralProfile(profile);
}

export function needsCanvas2d(profile: RenderProfile): boolean {
  return (
    profile === "average"
    || profile === "bicubic"
    || profile === "lanczos"
    || profile === "averageSharpWeak"
    || profile === "averageSharpStrong"
  );
}

export function cssImageRendering(profile: RenderProfile): "auto" | "pixelated" {
  if (profile === "nearest") return "pixelated";
  return "auto";
}
