<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { load } from "@tauri-apps/plugin-store";
import { getReaderStorePath } from "./lib/cacheDir";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import {
  APP_UI_SYNC_EVENT,
  clearLogs,
  fetchLogHistory,
  type AppUiSync,
  type LogEntry,
} from "./lib/appLog";
import { isLocale } from "./i18n";

const { t, locale } = useI18n();
const host = ref<HTMLElement | null>(null);

type InferProgress = {
  label: string;
  done: number;
  total: number;
  percent: number;
  finished: boolean;
};

function resolveOwner(): string {
  try {
    const fromQuery = new URLSearchParams(location.search).get("owner")?.trim();
    if (fromQuery) return fromQuery;
  } catch {
    // ignore
  }
  try {
    const label = getCurrentWindow().label || "";
    if (label.startsWith("log-")) return label.slice(4);
  } catch {
    // ignore
  }
  return "main";
}

const owner = resolveOwner();

type TermTheme = "dark" | "light";

const XTERM_THEMES = {
  dark: {
    background: "#0f1115",
    foreground: "#d6d8de",
    cursor: "#0f1115",
    selectionBackground: "#3a4458",
    scrollbarSliderBackground: "#ffffff33",
    scrollbarSliderHoverBackground: "#ffffff66",
    scrollbarSliderActiveBackground: "#ffffff80",
  },
  light: {
    background: "#f6f7f9",
    foreground: "#1a1d24",
    cursor: "#f6f7f9",
    selectionBackground: "#c5d4eb",
    scrollbarSliderBackground: "#00000022",
    scrollbarSliderHoverBackground: "#00000040",
    scrollbarSliderActiveBackground: "#00000055",
  },
} as const;

let term: Terminal | null = null;
let fit: FitAddon | null = null;
let unlistenLine: UnlistenFn | null = null;
let unlistenClear: UnlistenFn | null = null;
let unlistenUi: UnlistenFn | null = null;
let unlistenProgress: UnlistenFn | null = null;
let unlistenScroll: { dispose: () => void } | null = null;
let resizeObserver: ResizeObserver | null = null;
let progressOpen = false;
let follow = true;
let uiTheme: TermTheme = "dark";

function applyXtermTheme(theme: TermTheme) {
  if (!term) return;
  term.options.theme = { ...XTERM_THEMES[theme] };
}

function atBottom(): boolean {
  if (!term) return true;
  const buf = term.buffer.active;
  return buf.viewportY >= buf.baseY;
}

function syncFollowFromScroll() {
  follow = atBottom();
}

function stickIfFollowing() {
  if (follow && term) term.scrollToBottom();
}

function endProgressLine() {
  if (!progressOpen || !term) return;
  term.writeln("");
  progressOpen = false;
}

function writeProgress(progress: InferProgress) {
  if (!term) return;
  const total = Math.max(1, progress.total || 0);
  const done = Math.max(0, Math.min(progress.done, total));
  const percent = Math.max(0, Math.min(100, progress.percent ?? Math.round((done / total) * 100)));
  const width = 24;
  const filled = Math.round((percent / 100) * width);
  const bar = `${"█".repeat(filled)}${"░".repeat(width - filled)}`;
  const label = sanitize(progress.label || "");
  const line = `\x1b[36m[neural]\x1b[0m ${bar} ${String(percent).padStart(3, " ")}% (${done}/${total}) ${label}`;
  term.write(`\r\x1b[K${line}`);
  progressOpen = true;
  if (progress.finished || done >= total) {
    term.writeln("");
    progressOpen = false;
  }
  stickIfFollowing();
}

function levelColor(level: string): string {
  switch (level) {
    case "error":
      return "\x1b[31m";
    case "warn":
      return "\x1b[33m";
    case "info":
      return "\x1b[36m";
    case "debug":
      return "\x1b[90m";
    default:
      return "\x1b[37m";
  }
}

function sanitize(text: string): string {
  return text.replace(/\x1b\[[0-9;]*m/g, "").replace(/\r/g, "");
}

function formatTime(tsMs: number): string {
  const d = new Date(tsMs);
  if (Number.isNaN(d.getTime())) return "--:--:--";
  return d.toLocaleTimeString(undefined, { hour12: false });
}

function writeEntry(entry: LogEntry) {
  if (!term) return;
  endProgressLine();
  const color = levelColor(entry.level);
  const lines = sanitize(entry.message).split("\n");
  for (let i = 0; i < lines.length; i++) {
    const prefix =
      i === 0
        ? `${color}[${formatTime(entry.tsMs)}] [${entry.level}] [${entry.source}] `
        : `${color}  `;
    term.writeln(`${prefix}${lines[i]}\x1b[0m`);
  }
  stickIfFollowing();
}

async function onClear() {
  progressOpen = false;
  follow = true;
  await clearLogs(owner);
  term?.clear();
}

async function applyTheme(theme: TermTheme) {
  uiTheme = theme;
  document.documentElement.style.colorScheme = theme;
  document.documentElement.classList.toggle("theme-light", theme === "light");
  applyXtermTheme(theme);
  try {
    await getCurrentWindow().setTheme(theme);
  } catch {
    // Ignore when not in Tauri.
  }
}

async function applyLocale(next: string) {
  if (!isLocale(next)) return;
  locale.value = next;
  try {
    await getCurrentWindow().setTitle(t("logs"));
  } catch {
    // Ignore when not in Tauri.
  }
}

async function hydrateFromStore() {
  let theme: "dark" | "light" = "dark";
  try {
    const storePath = await getReaderStorePath();
    const store = await load(storePath, { autoSave: false, defaults: {} });
    const savedLocale = await store.get<string>("locale");
    if (isLocale(savedLocale)) locale.value = savedLocale;
    const savedSettings = await store.get<{ theme?: string }>("settings");
    if (savedSettings?.theme === "light") theme = "light";
  } catch {
    // Keep system locale / dark chrome fallback.
  }
  await applyTheme(theme);
  try {
    await getCurrentWindow().setTitle(t("logs"));
  } catch {
    // Non-Tauri / permission edge cases.
  }
}

onMounted(async () => {
  await hydrateFromStore();
  await nextTick();
  if (!host.value) return;

  term = new Terminal({
    convertEol: true,
    disableStdin: true,
    cursorBlink: false,
    fontSize: 13,
    fontFamily: "Consolas, 'Cascadia Mono', 'Courier New', monospace",
    theme: { ...XTERM_THEMES[uiTheme] },
    scrollback: 5000,
  });
  fit = new FitAddon();
  term.loadAddon(fit);
  term.open(host.value);
  fit.fit();
  // Canvas selection does not always raise a browser `copy` event in WebView2;
  // handle Ctrl/Cmd+C (and Ctrl+Insert) explicitly.
  term.attachCustomKeyEventHandler((ev) => {
    if (ev.type !== "keydown" || !term?.hasSelection()) return true;
    const key = ev.key.length === 1 ? ev.key.toLowerCase() : ev.key;
    const copyChord =
      ((ev.ctrlKey || ev.metaKey) && !ev.altKey && !ev.shiftKey && key === "c") ||
      (ev.ctrlKey && !ev.altKey && !ev.metaKey && !ev.shiftKey && ev.key === "Insert");
    if (!copyChord) return true;
    const text = term.getSelection();
    if (text) void navigator.clipboard.writeText(text).catch(() => {});
    return false;
  });
  unlistenScroll = term.onScroll(() => {
    syncFollowFromScroll();
  });

  const history = await fetchLogHistory(owner);
  for (const entry of history) writeEntry(entry);
  term.scrollToBottom();
  follow = true;

  unlistenLine = await listen<LogEntry>("app-log", (event) => {
    const entry = event.payload;
    if (entry.owner && entry.owner !== owner) return;
    writeEntry(entry);
  });
  unlistenClear = await listen("app-log-clear", () => {
    progressOpen = false;
    follow = true;
    term?.clear();
  });
  unlistenProgress = await listen<InferProgress>("neural-infer-progress", (event) => {
    writeProgress(event.payload);
  });
  unlistenUi = await listen<AppUiSync>(APP_UI_SYNC_EVENT, (event) => {
    const payload = event.payload ?? {};
    if (payload.theme === "dark" || payload.theme === "light") {
      void applyTheme(payload.theme);
    }
    if (typeof payload.locale === "string") {
      void applyLocale(payload.locale);
    }
  });

  resizeObserver = new ResizeObserver(() => {
    fit?.fit();
  });
  resizeObserver.observe(host.value);
});

onBeforeUnmount(() => {
  resizeObserver?.disconnect();
  unlistenScroll?.dispose();
  unlistenLine?.();
  unlistenClear?.();
  unlistenProgress?.();
  unlistenUi?.();
  term?.dispose();
  term = null;
  fit = null;
});
</script>

<template>
  <div class="log-app">
    <div ref="host" class="log-term" />
    <footer class="log-toolbar">
      <button type="button" class="hover-warning" @click="onClear">{{ t("logClear") }}</button>
    </footer>
  </div>
</template>

<style>
/* Mirror shared button tokens from styles.css — do not import the full sheet
   (it rewrites body/chrome and breaks the terminal look). */
:root {
  color-scheme: dark;
  font-family: "Segoe UI", "Microsoft YaHei", sans-serif;
  --ink: #f2f2f2;
  --line: #2c2c2c;
  --button: #1c1c1c;
  --accent: #409eff;
  --warning: #e6a23c;
  --chrome: #101010;
  --chrome-line: #2c2c2c;
  --term-bg: #0f1115;
  --term-fg: #d6d8de;
  --radius: 4px;
  --control-pad-y: 6px;
  --control-pad-x: 12px;
  --control-gap: 6px;
  --transition-fast:
    color 0.1s ease, background-color 0.1s ease, border-color 0.1s ease;
}

html.theme-light {
  color-scheme: light;
  --ink: #1a1a1a;
  --line: #e0e0e0;
  --button: #fff;
  --accent: #409eff;
  --warning: #e6a23c;
  --chrome: #f3f3f3;
  --chrome-line: #e0e0e0;
  --term-bg: #f6f7f9;
  --term-fg: #1a1d24;
}

html,
body,
#app {
  margin: 0;
  height: 100%;
  background: var(--term-bg);
  color: var(--term-fg);
  overflow: hidden;
}

button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: var(--control-gap);
  margin: 0;
  border: 1px solid var(--line);
  font: inherit;
  font-size: 14px;
  background: var(--button);
  color: var(--ink);
  border-radius: var(--radius);
  padding: var(--control-pad-y) var(--control-pad-x);
  line-height: 1.2;
  cursor: pointer;
  transition: var(--transition-fast);
}

button:hover:not(:disabled) {
  border-color: var(--accent);
  color: var(--accent);
}

button.hover-warning:hover:not(:disabled) {
  border-color: var(--warning);
  color: var(--warning);
}

.log-app {
  display: flex;
  flex-direction: column;
  height: 100%;
}

.log-toolbar {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 12px;
  padding: 8px 12px;
  border-top: 1px solid var(--chrome-line);
  background: var(--chrome);
  flex: 0 0 auto;
}

.log-term {
  flex: 1 1 auto;
  min-height: 0;
  min-width: 0;
  overflow: hidden;
  padding: 8px;
  box-sizing: border-box;
  background: var(--term-bg);
}

.log-term .xterm {
  height: 100%;
  width: 100%;
}

/* Layout overflow can leave an empty horizontal track under the terminal. */
.log-term .xterm-scrollable-element > .scrollbar.horizontal {
  display: none !important;
}
</style>
