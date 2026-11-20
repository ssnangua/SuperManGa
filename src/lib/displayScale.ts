import { ref } from "vue";

/** CSS px per device pixel inverse: Windows 150% → 1.5. Used so original size is 1 image pixel ≈ 1 screen pixel. */
export const screenScale = ref(
  typeof window !== "undefined" ? window.devicePixelRatio || 1 : 1,
);

export function syncDisplayScale() {
  if (typeof window === "undefined") return;
  const next = window.devicePixelRatio || 1;
  if (Math.abs(screenScale.value - next) < 0.0001) return;
  screenScale.value = next;
}

export function bindDisplayScale() {
  syncDisplayScale();
  window.addEventListener("resize", syncDisplayScale);
  window.visualViewport?.addEventListener("resize", syncDisplayScale);
}

export function cssFromImagePx(pixels: number, extraScale = 1) {
  const dpr = Math.max(0.01, screenScale.value);
  return (pixels * extraScale) / dpr;
}
