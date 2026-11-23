import type { MessageKey } from "../i18n";
import { isMacDesktop } from "./platform";

export type ShortcutActionId =
  | "openArchive"
  | "openFolder"
  | "openNewWindow"
  | "saveImage"
  | "copyImage"
  | "closeComic"
  | "closeWindow"
  | "settings"
  | "logs"
  | "theme"
  | "minimalist"
  | "colorAdjust"
  | "ocrTranslate"
  | "ocrRetranslate"
  | "fullscreen"
  | "panel"
  | "thumbnails"
  | "magnifier"
  | "bookmark"
  | "rotate"
  | "rotateCcw"
  | "play"
  | "first"
  | "prev"
  | "next"
  | "last"
  | "scrollUp"
  | "scrollDown";

export type ShortcutBindingMap = Record<ShortcutActionId, string>;

export type ShortcutAction = {
  id: ShortcutActionId;
  labelKey: MessageKey;
  /** Icon.vue asset name when the action has a matching toolbar/menu icon. */
  icon?: string;
  /** When false, the binding stays active but is omitted from the shortcuts list. */
  listed?: boolean;
  /** Only active when a comic is open. */
  needsComic?: boolean;
};

export const SHORTCUT_ACTIONS: readonly ShortcutAction[] = [
  { id: "openArchive", labelKey: "openArchive", icon: "open" },
  { id: "openFolder", labelKey: "openFolder", icon: "folderOpen" },
  { id: "closeComic", labelKey: "closeComic", needsComic: true },
  { id: "settings", labelKey: "settings", icon: "setting" },
  { id: "openNewWindow", labelKey: "openNewWindow", icon: "newWindow" },
  { id: "closeWindow", labelKey: "closeWindow" },
  { id: "ocrTranslate", labelKey: "enableTranslation", icon: "translate", needsComic: true },
  { id: "ocrRetranslate", labelKey: "retranslatePage", needsComic: true },
  { id: "magnifier", labelKey: "magnifier", icon: "lens", needsComic: true },
  { id: "copyImage", labelKey: "copyImage", icon: "copy", needsComic: true },
  { id: "saveImage", labelKey: "saveImageAs", icon: "save", needsComic: true },
  { id: "theme", labelKey: "shortcutTheme", icon: "themeDark" },
  { id: "panel", labelKey: "panel", icon: "panelOn", needsComic: true },
  { id: "first", labelKey: "first", icon: "first", needsComic: true },
  { id: "prev", labelKey: "prev", icon: "prev", needsComic: true },
  { id: "next", labelKey: "next", icon: "next", needsComic: true },
  { id: "last", labelKey: "last", icon: "last", needsComic: true },
  { id: "play", labelKey: "shortcutPlayToggle", icon: "play", needsComic: true },
  { id: "rotate", labelKey: "rotateCw", icon: "rotate", needsComic: true },
  { id: "rotateCcw", labelKey: "rotateCcw", icon: "rotate", needsComic: true },
  { id: "bookmark", labelKey: "shortcutBookmarkToggle", icon: "bookmarkOn", needsComic: true },
  { id: "thumbnails", labelKey: "shortcutThumbnailsToggle", icon: "thumb", needsComic: true },
  { id: "minimalist", labelKey: "minimalist", icon: "minimalist" },
  { id: "fullscreen", labelKey: "fullscreen", icon: "full" },
  /** Hidden from panel; binding + handler remain. */
  { id: "colorAdjust", labelKey: "colorAdjust", icon: "palette", listed: false },
  { id: "scrollUp", labelKey: "shortcutScrollUp", needsComic: true, listed: false },
  { id: "scrollDown", labelKey: "shortcutScrollDown", needsComic: true, listed: false },
  /** Fixed in shortcuts panel; still matched via DEFAULT_SHORTCUTS. */
  { id: "logs", labelKey: "logs", icon: "terminal", listed: false },
] as const;

/** Win/Linux: Ctrl+Shift+L; macOS: ⌘⌥L (browser-style, mirrors DevTools). */
export const FIXED_LOGS_ACCEL = isMacDesktop() ? "Command+Alt+L" : "Control+Shift+L";
/** Win/Linux: Ctrl+Shift+I; macOS: ⌘⌥I. */
export const FIXED_DEVTOOLS_ACCEL = isMacDesktop() ? "Command+Alt+I" : "Control+Shift+I";

/** Map Windows-style Control chords to Command on macOS (⌘O, ⌘S, …). */
function macifyPrimaryMod(accel: string): string {
  if (!isMacDesktop() || !accel.includes("Control")) return accel;
  return accel.split("Control").join("Command");
}

const DEFAULT_SHORTCUTS_WIN: ShortcutBindingMap = {
  openArchive: "Control+O",
  openFolder: "Control+Shift+O",
  openNewWindow: "Control+N",
  saveImage: "Control+S",
  copyImage: "Control+C",
  closeComic: "Control+W",
  closeWindow: "Control+Q",
  settings: "F5",
  logs: "Control+Shift+L",
  theme: "F2",
  minimalist: "F10",
  colorAdjust: "Control+U",
  ocrTranslate: "Control+T",
  ocrRetranslate: "Control+Shift+T",
  fullscreen: "F11",
  panel: "S",
  thumbnails: "T",
  magnifier: "Z",
  bookmark: "B",
  rotate: "R",
  rotateCcw: "Shift+R",
  play: "Space",
  first: "Home",
  prev: "PageUp",
  next: "PageDown",
  last: "End",
  scrollUp: "Up",
  scrollDown: "Down",
};

export const DEFAULT_SHORTCUTS: ShortcutBindingMap = {
  ...Object.fromEntries(
    Object.entries(DEFAULT_SHORTCUTS_WIN).map(([id, accel]) => [id, macifyPrimaryMod(accel)]),
  ) as ShortcutBindingMap,
  // Keep dedicated fixed chords (macOS uses ⌘⌥, not ⌘⇧).
  logs: FIXED_LOGS_ACCEL,
};

const MOD_ORDER = ["Control", "Command", "Alt", "Shift"] as const;
const NON_ACCELERATOR_KEYS = new Set(["Process", "Dead", "Unidentified"]);

const SHIFT_NEUTRAL_BY_CODE: Record<string, string> = {
  Digit0: "0",
  Digit1: "1",
  Digit2: "2",
  Digit3: "3",
  Digit4: "4",
  Digit5: "5",
  Digit6: "6",
  Digit7: "7",
  Digit8: "8",
  Digit9: "9",
  Minus: "-",
  Equal: "=",
  NumpadAdd: "=",
  NumpadSubtract: "-",
  BracketLeft: "[",
  BracketRight: "]",
  Backslash: "\\",
  Semicolon: ";",
  Quote: "'",
  Comma: ",",
  Period: ".",
  Slash: "/",
  Backquote: "`",
};

const KEY_ALIAS: Record<string, string> = {
  ctrl: "Control",
  control: "Control",
  cmd: "Command",
  command: "Command",
  meta: "Command",
  option: "Alt",
  alt: "Alt",
  shift: "Shift",
  space: "Space",
  " ": "Space",
  tab: "Tab",
  enter: "Enter",
  arrowleft: "Left",
  arrowright: "Right",
  arrowup: "Up",
  arrowdown: "Down",
  pageup: "PageUp",
  pagedown: "PageDown",
  esc: "Escape",
  escape: "Escape",
  return: "Enter",
  backspace: "Backspace",
  delete: "Delete",
  home: "Home",
  end: "End",
};

function normalizeToken(raw: string): string {
  const value = raw.trim();
  if (!value) return "";
  const alias = KEY_ALIAS[value.toLowerCase()];
  if (alias) return alias;
  if (value.length === 1) return value.toUpperCase();
  if (/^f\d{1,2}$/i.test(value)) return value.toUpperCase();
  return value[0]!.toUpperCase() + value.slice(1);
}

function tokenizeAccelerator(raw: string): string[] {
  return String(raw ?? "")
    .split("+")
    .map(normalizeToken)
    .filter(Boolean);
}

export function normalizeAccelerator(raw: string): string {
  const parts = tokenizeAccelerator(raw);
  const mods = new Set<string>();
  let key = "";
  for (const part of parts) {
    if ((MOD_ORDER as readonly string[]).includes(part)) mods.add(part);
    else key = part;
  }
  const ordered = MOD_ORDER.filter((mod) => mods.has(mod));
  return key ? [...ordered, key].join("+") : "";
}

function physicalKeyFromCode(event: KeyboardEvent): string | null {
  const { code } = event;
  if (!code || code === "Unidentified") return null;
  if (code.startsWith("Digit")) {
    const digit = code.slice(5);
    if (/^\d$/.test(digit)) return digit;
  }
  if (code.startsWith("Key")) {
    const letter = code.slice(3);
    if (/^[A-Z]$/.test(letter)) return letter;
  }
  const functionKey = /^F(\d{1,2})$/.exec(code);
  if (functionKey) return `F${functionKey[1]}`;
  if (code === "Space") return "Space";
  if (code === "ArrowUp") return "Up";
  if (code === "ArrowDown") return "Down";
  if (code === "ArrowLeft") return "Left";
  if (code === "ArrowRight") return "Right";
  if (code === "PageUp") return "PageUp";
  if (code === "PageDown") return "PageDown";
  if (code === "Home") return "Home";
  if (code === "End") return "End";
  if (code === "Escape") return "Escape";
  if (code === "Tab") return "Tab";
  if (code === "Enter") return "Enter";
  if (code === "Backspace") return "Backspace";
  if (code === "Delete") return "Delete";
  const punct = SHIFT_NEUTRAL_BY_CODE[code];
  if (punct !== undefined) return punct;
  return null;
}

function keyFromKeyboardEvent(event: KeyboardEvent): string {
  const fromCode = physicalKeyFromCode(event);
  if (fromCode) return fromCode;
  const key = normalizeToken(event.key);
  if (key === " ") return "Space";
  return key;
}

export function keyboardEventToAccelerator(event: KeyboardEvent): string {
  if (event.isComposing || NON_ACCELERATOR_KEYS.has(String(event.key || ""))) return "";
  const mods: string[] = [];
  if (event.ctrlKey) mods.push("Control");
  if (event.metaKey) mods.push("Command");
  if (event.altKey) mods.push("Alt");
  if (event.shiftKey) mods.push("Shift");
  const key = keyFromKeyboardEvent(event);
  if (!key || (MOD_ORDER as readonly string[]).includes(key)) return "";
  return normalizeAccelerator([...mods, key].join("+"));
}

export function isListedShortcut(id: ShortcutActionId): boolean {
  return SHORTCUT_ACTIONS.find((action) => action.id === id)?.listed !== false;
}

export function mergeShortcutBindings(
  defaults: ShortcutBindingMap,
  loaded?: Partial<Record<string, string>>,
): ShortcutBindingMap {
  const next = { ...defaults };
  if (!loaded) return next;
  for (const action of SHORTCUT_ACTIONS) {
    if (!isListedShortcut(action.id)) continue;
    const raw = loaded[action.id];
    if (typeof raw !== "string") continue;
    const normalized = normalizeAccelerator(raw);
    if (normalized) next[action.id] = normalized;
  }
  return next;
}

export function shortcutOverrides(
  defaults: ShortcutBindingMap,
  bindings: ShortcutBindingMap,
): Partial<Record<ShortcutActionId, string>> {
  const out: Partial<Record<ShortcutActionId, string>> = {};
  for (const action of SHORTCUT_ACTIONS) {
    if (!isListedShortcut(action.id)) continue;
    const current = normalizeAccelerator(bindings[action.id]);
    const fallback = normalizeAccelerator(defaults[action.id]);
    if (current && current !== fallback) out[action.id] = current;
  }
  return out;
}

export function collectShortcutConflicts(bindings: ShortcutBindingMap): ShortcutActionId[] {
  const first = new Map<string, ShortcutActionId>();
  const conflicts: ShortcutActionId[] = [];
  for (const action of SHORTCUT_ACTIONS) {
    const accel = normalizeAccelerator(bindings[action.id]);
    if (!accel) continue;
    const previous = first.get(accel);
    if (!previous) {
      first.set(accel, action.id);
      continue;
    }
    if (!conflicts.includes(previous)) conflicts.push(previous);
    conflicts.push(action.id);
  }
  return conflicts;
}

export function acceleratorToDisplayKeys(accel: string, isMac = isMacDesktop()): string[] {
  return tokenizeAccelerator(accel).map((part) => {
    if (part === "Control") return "Ctrl";
    if (part === "Command") return isMac ? "Cmd" : "Win";
    if (part === "Alt") return isMac ? "Option" : "Alt";
    if (part === "Shift") return "Shift";
    if (part === "Left") return "←";
    if (part === "Right") return "→";
    if (part === "Up") return "↑";
    if (part === "Down") return "↓";
    return part;
  });
}

export function acceleratorToDisplayText(accel: string, isMac = isMacDesktop()): string {
  return acceleratorToDisplayKeys(accel, isMac).join("+");
}

export function titleWithShortcut(label: string, accel: string, isMac = isMacDesktop()): string {
  const text = acceleratorToDisplayText(accel, isMac).trim();
  return text ? `${label} (${text})` : label;
}

export function matchShortcutAction(
  accel: string,
  bindings: ShortcutBindingMap,
): ShortcutActionId | null {
  const normalized = normalizeAccelerator(accel);
  if (!normalized) return null;
  for (const action of SHORTCUT_ACTIONS) {
    if (normalizeAccelerator(bindings[action.id]) === normalized) return action.id;
  }
  return null;
}
