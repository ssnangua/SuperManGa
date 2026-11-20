/** Swallow the first canvas tap after the window was unfocused (click-to-activate). */

let armed = false;
let focusedFrame = typeof document !== "undefined" ? document.hasFocus() : true;
let clearTimer = 0;
let started = false;

const GRACE_MS = 400;

function arm() {
  armed = true;
  focusedFrame = false;
  window.clearTimeout(clearTimer);
}

function scheduleDisarm() {
  window.clearTimeout(clearTimer);
  clearTimer = window.setTimeout(() => {
    armed = false;
  }, GRACE_MS);
}

function onWindowBlur() {
  arm();
}

function onWindowFocus() {
  scheduleDisarm();
}

function onVisibility() {
  if (document.visibilityState === "hidden") arm();
}

function trackFocus() {
  focusedFrame = document.hasFocus();
  requestAnimationFrame(trackFocus);
}

/** Call once; safe to call from every stage binder. */
export function ensureFocusTapGuard() {
  if (started) return;
  started = true;
  focusedFrame = document.hasFocus();
  requestAnimationFrame(trackFocus);
  window.addEventListener("blur", onWindowBlur);
  window.addEventListener("focus", onWindowFocus);
  document.addEventListener("visibilitychange", onVisibility);
  if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
    void import("@tauri-apps/api/window").then(({ getCurrentWindow }) => {
      void getCurrentWindow()
        .onFocusChanged(({ payload: focused }) => {
          if (focused) scheduleDisarm();
          else arm();
        })
        .catch(() => {
          // Permission or runtime missing; DOM listeners remain.
        });
    });
  }
}

/** If the window was just unfocused, consume the guard and skip this tap. */
export function consumeFocusTapSuppress(): boolean {
  const skip = armed || !focusedFrame;
  if (!skip) return false;
  armed = false;
  focusedFrame = true;
  window.clearTimeout(clearTimer);
  return true;
}
