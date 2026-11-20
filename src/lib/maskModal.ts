import { computed, shallowRef } from "vue";

/** Matches `--z-modal` in styles.css — stacked masks use base + layer index. */
export const MASK_MODAL_BASE_Z = 100;

/** Floating UI owned by a mask modal (select / tip / color picker). Above top mask. */
export function maskModalPopoverZIndex(): number {
  return MASK_MODAL_BASE_Z + Math.max(1, stack.value.length) + 20;
}

/** Global toasts while a mask modal is open — above modal popovers. */
export function maskModalToastZIndex(): number {
  return maskModalPopoverZIndex() + 10;
}

export type MaskModalRegistration = {
  /** Stable id across HMR remounts so layer order is preserved. */
  key: string;
  close: () => void;
  /** When false, Esc is consumed while this is topmost and does not close lower modals. */
  closeOnEsc?: boolean;
};

type MaskModalEntry = {
  key: string;
  close: () => void;
  closeOnEsc: boolean;
};

const stack = shallowRef<MaskModalEntry[]>([]);
/** Deferred removals so HMR unmount→remount of the same key keeps its stack slot. */
const pendingRemove = new Map<string, ReturnType<typeof setTimeout>>();

function flushRemove(key: string) {
  pendingRemove.delete(key);
  if (!stack.value.some((item) => item.key === key)) return;
  stack.value = stack.value.filter((item) => item.key !== key);
}

/** Register an open mask modal; returns unregister. Later registrations sit above earlier ones. */
export function registerMaskModal(registration: MaskModalRegistration): () => void {
  const key = registration.key;
  const pending = pendingRemove.get(key);
  if (pending !== undefined) {
    clearTimeout(pending);
    pendingRemove.delete(key);
  }

  const entry: MaskModalEntry = {
    key,
    close: registration.close,
    closeOnEsc: registration.closeOnEsc !== false,
  };

  const existing = stack.value.findIndex((item) => item.key === key);
  if (existing >= 0) {
    const next = stack.value.slice();
    next[existing] = entry;
    stack.value = next;
  } else {
    stack.value = [...stack.value, entry];
  }

  return () => {
    if (pendingRemove.has(key)) return;
    // Defer so a same-key remount in this turn can replace in place (HMR).
    pendingRemove.set(
      key,
      setTimeout(() => {
        flushRemove(key);
      }, 0),
    );
  };
}

/** Close the topmost Esc-closable mask modal. Returns true if Esc was handled. */
export function closeTopMaskModal(): boolean {
  const list = stack.value;
  if (list.length === 0) return false;
  const top = list[list.length - 1]!;
  if (!top.closeOnEsc) return true;
  top.close();
  return true;
}

/** Stack layer index for a registered key (0 = bottom). -1 if absent. */
export function maskModalLayerIndex(key: string): number {
  return stack.value.findIndex((item) => item.key === key);
}

/** CSS z-index for a registered mask key. */
export function maskModalZIndex(key: string): number {
  const layer = maskModalLayerIndex(key);
  return MASK_MODAL_BASE_Z + Math.max(0, layer);
}

/** Any registered mask modal is open (for chrome menu suppression, etc.). */
export const maskModalOpen = computed(() => stack.value.length > 0);
