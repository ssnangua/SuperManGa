import { isAppMenuOpen } from "./appMenu";
import { consumeFocusTapSuppress, ensureFocusTapGuard } from "./focusTap";

const DRAG_SLOP = 4;
export const GLIDE_MS = 140;

export function glideEase(t: number): number {
  const clamped = Math.max(0, Math.min(1, t));
  return 1 - (1 - clamped) * (1 - clamped);
}

export function reducedMotion() {
  return typeof window !== "undefined" && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

function overflows(el: HTMLElement) {
  return {
    x: el.scrollWidth - el.clientWidth > 1,
    y: el.scrollHeight - el.clientHeight > 1,
  };
}

function limitOf(el: HTMLElement, axis: "top" | "left") {
  return axis === "top"
    ? el.scrollHeight - el.clientHeight
    : el.scrollWidth - el.clientWidth;
}

function readingOf(el: HTMLElement, axis: "top" | "left") {
  return axis === "top" ? el.scrollTop : el.scrollLeft;
}

function atStart(el: HTMLElement, axis: "top" | "left") {
  return readingOf(el, axis) <= 1;
}

function atEnd(el: HTMLElement, axis: "top" | "left") {
  return readingOf(el, axis) >= limitOf(el, axis) - 1;
}

export type StageStep = {
  axis: "top" | "left";
  delta: number;
  resetLeft?: number;
};

/** Next viewport step: horizontal first, then vertical. Null means turn the page. */
export function planStageStep(el: HTMLElement, forward: boolean, rtl = false): StageStep | null {
  const bounds = overflows(el);
  if (!bounds.x && !bounds.y) return null;

  if (rtl) {
    if (forward) {
      if (bounds.x && !atStart(el, "left")) return { axis: "left", delta: -el.clientWidth };
      if (bounds.y && !atEnd(el, "top")) {
        return {
          axis: "top",
          delta: el.clientHeight,
          resetLeft: bounds.x ? limitOf(el, "left") : undefined,
        };
      }
      return null;
    }
    if (bounds.x && !atEnd(el, "left")) return { axis: "left", delta: el.clientWidth };
    if (bounds.y && !atStart(el, "top")) {
      return {
        axis: "top",
        delta: -el.clientHeight,
        resetLeft: bounds.x ? 0 : undefined,
      };
    }
    return null;
  }

  if (forward) {
    if (bounds.x && !atEnd(el, "left")) return { axis: "left", delta: el.clientWidth };
    if (bounds.y && !atEnd(el, "top")) {
      return {
        axis: "top",
        delta: el.clientHeight,
        resetLeft: bounds.x ? 0 : undefined,
      };
    }
    return null;
  }

  if (bounds.x && !atStart(el, "left")) return { axis: "left", delta: -el.clientWidth };
  if (bounds.y && !atStart(el, "top")) {
    return {
      axis: "top",
      delta: -el.clientHeight,
      resetLeft: bounds.x ? limitOf(el, "left") : undefined,
    };
  }
  return null;
}

let stageAnimation = 0;

export function stopStageAnimation() {
  if (!stageAnimation) return;
  cancelAnimationFrame(stageAnimation);
  stageAnimation = 0;
}

function glideStage(el: HTMLElement, axis: "top" | "left", delta: number, animate = true): boolean {
  stopStageAnimation();
  const reading = readingOf(el, axis);
  const target = Math.max(0, Math.min(limitOf(el, axis), reading + delta));
  const distance = target - reading;
  if (Math.abs(distance) < 1) return false;
  if (!animate || reducedMotion()) {
    if (axis === "top") el.scrollTop = target;
    else el.scrollLeft = target;
    return true;
  }
  const started = performance.now();
  const frame = () => {
    const eased = glideEase((performance.now() - started) / GLIDE_MS);
    const value = reading + distance * eased;
    if (axis === "top") el.scrollTop = value;
    else el.scrollLeft = value;
    stageAnimation = eased < 1 ? requestAnimationFrame(frame) : 0;
  };
  stageAnimation = requestAnimationFrame(frame);
  return true;
}

/** Apply one viewport scroll step with the same glide as a click. Returns false when the page should turn. */
export function stepStageScroll(el: HTMLElement, forward: boolean, rtl = false, animate = true): boolean {
  const step = planStageStep(el, forward, rtl);
  if (!step) return false;
  if (step.resetLeft != null) el.scrollLeft = step.resetLeft;
  return glideStage(el, step.axis, step.delta, animate);
}

export function stageOverflows(el: HTMLElement) {
  const bounds = overflows(el);
  return bounds.x || bounds.y;
}

export function remainingStageScroll(el: HTMLElement, forward: boolean, rtl = false): number {
  const step = planStageStep(el, forward, rtl);
  if (!step) return 0;
  const reading = readingOf(el, step.axis);
  const limit = limitOf(el, step.axis);
  return step.delta > 0 ? Math.max(0, limit - reading) : Math.max(0, reading);
}

export function nudgeStageScroll(
  el: HTMLElement,
  px: number,
  forward: boolean,
  rtl = false,
  animate = false,
): boolean {
  const step = planStageStep(el, forward, rtl);
  if (!step) return false;
  const remaining = remainingStageScroll(el, forward, rtl);
  if (remaining < 1) return false;
  const mag = Math.min(Math.max(1, px), remaining);
  const signed = (step.delta > 0 ? 1 : -1) * mag;
  if (step.resetLeft != null) el.scrollLeft = step.resetLeft;
  return glideStage(el, step.axis, signed, animate);
}

export function scrollStageOrigin(el: HTMLElement, rtl = false) {
  stopStageAnimation();
  el.scrollTop = 0;
  el.scrollLeft = rtl ? Math.max(0, limitOf(el, "left")) : 0;
}

function onScrollbar(el: HTMLElement, event: PointerEvent): boolean {
  const bounds = overflows(el);
  const rect = el.getBoundingClientRect();
  const x = event.clientX - rect.left;
  const y = event.clientY - rect.top;
  if (bounds.y && x >= el.clientLeft + el.clientWidth) return true;
  if (bounds.x && y >= el.clientTop + el.clientHeight) return true;
  return false;
}

export function bindStagePointer(
  el: HTMLElement,
  turn: (forward: boolean) => void,
  rtl: () => boolean = () => false,
  animate: () => boolean = () => true,
  onDrag?: (active: boolean) => void,
): () => void {
  ensureFocusTapGuard();

  let active = false;
  let moved = false;
  let dragging = false;
  let skipTap = false;
  let button = 0;
  let pointerId = -1;
  let startX = 0;
  let startY = 0;
  let originLeft = 0;
  let originTop = 0;
  let swallowClick = false;
  let previousCursor = "";
  let cursorChanged = false;

  function resetCursor() {
    if (!cursorChanged) return;
    document.documentElement.style.cursor = previousCursor;
    document.documentElement.classList.remove("grabbing");
    cursorChanged = false;
  }

  function activate(forward: boolean) {
    const step = planStageStep(el, forward, rtl());
    if (!step) {
      turn(forward);
      return;
    }
    if (step.resetLeft != null) el.scrollLeft = step.resetLeft;
    if (!glideStage(el, step.axis, step.delta, animate())) turn(forward);
  }

  function onDown(event: PointerEvent) {
    if (event.pointerType !== "mouse" || (event.button !== 0 && event.button !== 2)) return;
    if (onScrollbar(el, event)) {
      swallowClick = false;
      return;
    }
    // Dismiss chrome menus without turning the page / scrolling the stage.
    if (isAppMenuOpen()) {
      active = false;
      swallowClick = false;
      return;
    }
    skipTap = consumeFocusTapSuppress();
    stopStageAnimation();
    onDrag?.(true);
    active = true;
    moved = false;
    dragging = false;
    button = event.button;
    pointerId = event.pointerId;
    const bounds = overflows(el);
    if (bounds.x || bounds.y) {
      dragging = true;
      startX = event.clientX;
      startY = event.clientY;
      originLeft = el.scrollLeft;
      originTop = el.scrollTop;
    }
    try {
      el.setPointerCapture(pointerId);
    } catch {
      // Synthetic events and a lost pointer have nothing to capture.
    }
    event.preventDefault();
  }

  function onMove(event: PointerEvent) {
    if (!dragging || !active || event.pointerId !== pointerId) return;
    const dx = event.clientX - startX;
    const dy = event.clientY - startY;
    if (!moved && dx * dx + dy * dy < DRAG_SLOP * DRAG_SLOP) return;
    if (!moved) {
      moved = true;
      previousCursor = document.documentElement.style.cursor;
      document.documentElement.style.cursor = "grabbing";
      document.documentElement.classList.add("grabbing");
      cursorChanged = true;
    }
    el.scrollLeft = originLeft - dx;
    el.scrollTop = originTop - dy;
  }

  function finish(event: PointerEvent, tap: boolean) {
    if (!active || event.pointerId !== pointerId) return;
    active = false;
    resetCursor();
    if (el.hasPointerCapture(pointerId)) el.releasePointerCapture(pointerId);
    swallowClick = button === 0;
    if (tap && !moved && !skipTap) activate(button === 0);
    skipTap = false;
    onDrag?.(false);
  }

  function onUp(event: PointerEvent) {
    finish(event, true);
  }

  function onCancel(event: PointerEvent) {
    finish(event, false);
  }

  function onClick(event: MouseEvent) {
    if (!swallowClick) return;
    swallowClick = false;
    event.preventDefault();
    event.stopPropagation();
  }

  function onContextMenu(event: MouseEvent) {
    event.preventDefault();
  }

  el.addEventListener("pointerdown", onDown);
  el.addEventListener("pointermove", onMove);
  el.addEventListener("pointerup", onUp);
  el.addEventListener("pointercancel", onCancel);
  el.addEventListener("click", onClick, true);
  el.addEventListener("contextmenu", onContextMenu);

  return () => {
    stopStageAnimation();
    if (active) resetCursor();
    el.removeEventListener("pointerdown", onDown);
    el.removeEventListener("pointermove", onMove);
    el.removeEventListener("pointerup", onUp);
    el.removeEventListener("pointercancel", onCancel);
    el.removeEventListener("click", onClick, true);
    el.removeEventListener("contextmenu", onContextMenu);
  };
}
