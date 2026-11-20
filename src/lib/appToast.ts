import { reactive } from "vue";

export type AppToastKind = "none" | "success" | "info" | "warning" | "error";

export type AppToastItem = {
  id: number;
  message: string;
  kind: AppToastKind;
  showClose: boolean;
};

export type AppToastOptions = {
  kind?: AppToastKind;
  /**
   * Show a dismiss button. Defaults to `true` for `error`, otherwise `false`.
   */
  showClose?: boolean;
  /**
   * Auto-dismiss delay in ms. Default 3000.
   * `error` defaults to `0` (keep until dismissed).
   * Pass `0` to keep until dismissed.
   */
  duration?: number;
};

const DEFAULT_DURATION_MS = 3000;

export const appToastItems = reactive<AppToastItem[]>([]);

let idSeq = 0;
const timers = new Map<number, ReturnType<typeof setTimeout>>();

export function dismissAppToast(id: number): void {
  const timer = timers.get(id);
  if (timer !== undefined) {
    clearTimeout(timer);
    timers.delete(id);
  }
  const index = appToastItems.findIndex((item) => item.id === id);
  if (index >= 0) appToastItems.splice(index, 1);
}

export function clearAppToasts(kind?: AppToastKind): void {
  const ids = appToastItems
    .filter((item) => kind == null || item.kind === kind)
    .map((item) => item.id);
  for (const id of ids) dismissAppToast(id);
}
export function appToast(message: string, options?: AppToastOptions): number {
  const text = String(message ?? "").trim();
  if (!text) return -1;
  const kind = options?.kind ?? "none";
  const isError = kind === "error";
  const showClose = options?.showClose ?? isError;
  const duration = options?.duration ?? (isError ? 0 : DEFAULT_DURATION_MS);
  const id = ++idSeq;
  appToastItems.push({ id, message: text, kind, showClose });
  if (duration > 0) {
    const timer = setTimeout(() => dismissAppToast(id), duration);
    timers.set(id, timer);
  }
  return id;
}

export function notifySuccess(message: string, duration = DEFAULT_DURATION_MS): number {
  return appToast(message, { kind: "success", duration });
}

/** Error toasts stay until the user dismisses them (close button). */
export function notifyError(message: string): number {
  return appToast(message, { kind: "error" });
}

export function notifyInfo(message: string, duration = DEFAULT_DURATION_MS): number {
  return appToast(message, { kind: "info", duration });
}

export function notifyWarning(message: string, duration = DEFAULT_DURATION_MS): number {
  return appToast(message, { kind: "warning", duration });
}
