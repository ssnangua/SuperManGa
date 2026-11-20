import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

export type LogLevel = "log" | "info" | "warn" | "error" | "debug";

export type LogEntry = {
  id: number;
  tsMs: number;
  level: string;
  source: string;
  message: string;
  owner?: string;
};

export type AppUiSync = {
  theme?: "dark" | "light";
  locale?: string;
};

export const APP_UI_SYNC_EVENT = "app-ui-sync";

let bridged = false;
let windowSource = "main";
/** Set on pagehide/beforeunload — unload IPC noise must not spam the log window. */
let unloading = false;

function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function isLogWindowLabel(label: string): boolean {
  return label.startsWith("log-") || label === "log";
}

function isUnloadIpcNoise(message: string): boolean {
  return message.includes("IPC custom protocol failed");
}

function formatArg(arg: unknown): string {
  if (typeof arg === "string") return arg;
  if (arg instanceof Error) return arg.stack || arg.message || String(arg);
  if (typeof arg === "undefined") return "undefined";
  if (typeof arg === "symbol") return arg.toString();
  try {
    return JSON.stringify(arg);
  } catch {
    return String(arg);
  }
}

function formatArgs(args: unknown[]): string {
  return args.map(formatArg).join(" ");
}

function push(level: LogLevel, message: string) {
  if (!inTauri() || !message || unloading || isUnloadIpcNoise(message)) return;
  void invoke("append_log", {
    level,
    source: windowSource,
    message,
    owner: windowSource,
  }).catch(() => {
    // Ignore bridge failures (e.g. before backend is ready).
  });
}

/** Hook console.* so reader windows share one log buffer with the log window. */
export function installConsoleBridge() {
  if (bridged || !inTauri()) return;
  try {
    windowSource = getCurrentWindow().label || "main";
  } catch {
    windowSource = "main";
  }
  // Log window only displays; do not re-bridge its console.
  if (isLogWindowLabel(windowSource)) {
    bridged = true;
    return;
  }

  const markUnloading = () => {
    unloading = true;
  };
  window.addEventListener("pagehide", markUnloading);
  window.addEventListener("beforeunload", markUnloading);

  const levels: LogLevel[] = ["log", "info", "warn", "error", "debug"];
  for (const level of levels) {
    const original = console[level].bind(console);
    console[level] = (...args: unknown[]) => {
      original(...args);
      push(level, formatArgs(args));
    };
  }
  bridged = true;
}

export async function openLogWindow(
  owner: string,
  title: string,
  theme: "dark" | "light" = "dark",
) {
  if (!inTauri()) return;
  await invoke("open_log_window", { owner, title, theme });
}

/** Sync theme/locale to the paired log window only. */
export async function broadcastUiSync(payload: AppUiSync) {
  if (!inTauri()) return;
  try {
    const owner = getCurrentWindow().label || "main";
    if (isLogWindowLabel(owner)) return;
    await invoke("sync_log_window_ui", {
      owner,
      theme: payload.theme,
      locale: payload.locale,
    });
  } catch {
    // Ignore when backend/event bus is unavailable.
  }
}

export async function fetchLogHistory(owner: string): Promise<LogEntry[]> {
  if (!inTauri()) return [];
  return invoke<LogEntry[]>("get_log_history", { owner });
}

export async function clearLogs(owner: string) {
  if (!inTauri()) return;
  await invoke("clear_logs", { owner });
}
