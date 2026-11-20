// A pointer gap between items is shorter than this. Clearing immediately would restart the fade-in.
const LEAVE_MS = 50;

let leaveTimer = 0;
let leaveTarget: HTMLButtonElement | null = null;

function menuButton(target: EventTarget | null): HTMLButtonElement | null {
  if (!(target instanceof Element)) return null;
  const button = target.closest("button");
  if (!(button instanceof HTMLButtonElement) || button.disabled) return null;
  if (!button.closest(".bar-menu")) return null;
  return button;
}

function cancelLeave() {
  window.clearTimeout(leaveTimer);
  leaveTarget = null;
}

function onOver(event: PointerEvent) {
  const button = menuButton(event.target);
  if (!button) return;
  if (event.relatedTarget instanceof Node && button.contains(event.relatedTarget)) return;
  if (leaveTarget && leaveTarget !== button) leaveTarget.classList.remove("is-hot");
  cancelLeave();
  const menu = button.closest(".bar-menu");
  menu?.querySelectorAll<HTMLElement>(".is-hot").forEach((item) => {
    if (item !== button) item.classList.remove("is-hot");
  });
  button.classList.add("is-hot");
}

function onOut(event: PointerEvent) {
  const button = menuButton(event.target);
  if (!button?.classList.contains("is-hot")) return;
  if (event.relatedTarget instanceof Node && button.contains(event.relatedTarget)) return;
  cancelLeave();
  leaveTarget = button;
  leaveTimer = window.setTimeout(() => {
    leaveTarget?.classList.remove("is-hot");
    leaveTarget = null;
  }, LEAVE_MS);
}

export function bindMenuHot(): () => void {
  document.addEventListener("pointerover", onOver);
  document.addEventListener("pointerout", onOut);
  return () => {
    document.removeEventListener("pointerover", onOver);
    document.removeEventListener("pointerout", onOut);
    cancelLeave();
  };
}
