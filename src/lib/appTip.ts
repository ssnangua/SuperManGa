import { shallowRef } from "vue";

export type AppTipPlacement = "above" | "below";

export type AppTipModel = {
  text: string;
  /** Viewport X of the tip's preferred center. */
  x: number;
  /** Viewport Y of the anchor edge (button top for above, bottom for below). */
  y: number;
  placement: AppTipPlacement;
  gap?: number;
};

/** Single floating tip teleported to body (escapes toolbar/seek stacking). */
export const appTip = shallowRef<AppTipModel | null>(null);

export function showAppTip(model: AppTipModel) {
  appTip.value = model;
}

export function hideAppTip() {
  appTip.value = null;
}
