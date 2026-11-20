const SKIP_INPUT_TYPES = new Set([
  "button",
  "checkbox",
  "radio",
  "file",
  "submit",
  "reset",
  "hidden",
  "image",
  "range",
  "color",
]);

/** Text-like fields where Esc should blur instead of closing chrome/modals. */
export function isEscapeBlurTextField(
  target: EventTarget | null,
): target is HTMLElement {
  if (!(target instanceof HTMLElement)) return false;
  const tag = target.tagName;
  if (tag === "TEXTAREA") return true;
  if (tag === "INPUT") {
    const type = ((target as HTMLInputElement).type || "text").toLowerCase();
    return !SKIP_INPUT_TYPES.has(type);
  }
  return target.isContentEditable;
}

/** @returns true if Esc was consumed by blurring the focused text field. */
export function tryBlurFocusedTextFieldOnEscape(ev: KeyboardEvent): boolean {
  if (ev.key !== "Escape" || ev.repeat) return false;
  if (!isEscapeBlurTextField(ev.target)) return false;
  ev.preventDefault();
  ev.stopImmediatePropagation();
  ev.target.blur();
  return true;
}

let installed = false;

/** Capture-phase listener; install early so it wins over modal / chrome Esc handlers. */
export function installEscapeBlurTextFieldListener(): void {
  if (installed) return;
  if (typeof document === "undefined") return;
  installed = true;
  document.addEventListener(
    "keydown",
    (ev) => {
      tryBlurFocusedTextFieldOnEscape(ev);
    },
    true,
  );
}
