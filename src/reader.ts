import { computed, reactive, ref, toRef, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { confirm, open, save } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { load, type Store } from "@tauri-apps/plugin-store";
import { getCacheDir, getReaderStorePath } from "./lib/cacheDir";
import {
  hasReadyOcrEngine,
  pickReadyOcrEngine,
  type OcrReadinessContext,
} from "./lib/ocr/availability";
import { installedOcrAssetIds, requiredOcrAssetIdsForEngine } from "./lib/ocr/assets";
import { normalizeOcrEngineId } from "./lib/ocr/engines";
import type { OcrPageResult, OcrRegion } from "./lib/ocr/types";
import {
  nearFocusTranslatePages,
  rankTranslatePages,
  TRANSLATE_PRIORITY_BATCH,
  translatePriorityForIndex,
} from "./lib/ocr/translateQueue";
import { neuralRuntimeStatus } from "./lib/render/assets";
import { defaultTranslationSettings, mergeTranslationSettings } from "./lib/translation/defaults";
import { formatTranslationError, isTranslationErrorCode } from "./lib/translation/errors";
import {
  hydrateTranslationSecretsFromVault,
  persistTranslationSecretsToVault,
  stripTranslationSecretsForDisk,
  syncAiApiKeyIntoVendorSlot,
} from "./lib/translation/secrets";
import type { TranslationSettings } from "./lib/translation/types";
import { detectSystemLocale, i18n, isLocale, type Locale } from "./i18n";
import { buildChapters } from "./lib/chapters";
import {
  cloneChapterRules,
  compileEnabledChapterRules,
  createDefaultChapterRules,
  resolveChapterRules,
  type ChapterFilenameRule,
} from "./lib/chapterRules";
import { pageUrl, setDemoUrls } from "./lib/pages";
import { spreadIndexes, spreadStart, type ReadingMode } from "./lib/spread";
import { ensureFocusTapGuard } from "./lib/focusTap";
import { nudgeStageScroll, planStageStep, remainingStageScroll, stageOverflows } from "./lib/stagePointer";
import { parseBookmarks, serializeBookmarks, type Bookmark } from "./lib/bookmarks";
import { bindDisplayScale, screenScale } from "./lib/displayScale";
import { initWindowState, persistWindowState } from "./lib/windowState";
import {
  isNeuralProfile,
  normalizeRenderProfile,
  type RenderProfile,
} from "./lib/render/profile";
import { neuralBatchPaused, setNeuralFocus } from "./lib/render/neural";
import { syncNeuralBook } from "./lib/render/neuralBook";
import {
  clearAppToasts,
  notifyError,
  notifyInfo,
  notifySuccess,
  notifyWarning,
} from "./lib/appToast";
import { closeTopMaskModal, maskModalOpen } from "./lib/maskModal";
import {
  DEFAULT_SHORTCUTS,
  FIXED_DEVTOOLS_ACCEL,
  FIXED_LOGS_ACCEL,
  acceleratorToDisplayText,
  keyboardEventToAccelerator,
  isListedShortcut,
  matchShortcutAction,
  mergeShortcutBindings,
  normalizeAccelerator,
  shortcutOverrides,
  titleWithShortcut,
  type ShortcutActionId,
  type ShortcutBindingMap,
} from "./lib/shortcuts";

export interface PageInfo {
  name: string;
  relPath: string;
  width: number;
  height: number;
}

export interface Comic {
  sessionId: number;
  title: string;
  path: string;
  pages: PageInfo[];
  extracting: boolean;
  initialIndex: number | null;
  protocolOrigin: string;
}

export interface HistoryItem {
  path: string;
  index: number;
  offset: number;
  title: string;
}

export type ReadingDirection = "ltr" | "rtl";

export interface ModeFitSettings {
  fitMode: FitMode;
  fitFixedWidth: number;
  fitOriginalScale: number;
  direction: ReadingDirection;
}

export type ModeFits = Record<ReadingMode, ModeFitSettings>;

export type ScrollTrackBehavior = "off" | "page" | "jump";
export type SpreadAlign = "top" | "center" | "bottom";
export type PlayScrollMode = "smooth" | "interval";
export type PlayIntervalStepMode = "screen" | "custom";

export const ASSOC_GROUPS = [
  { id: "comic", labelKey: "assocComic", exts: ["cbz", "cbr"] },
  { id: "archive", labelKey: "assocArchive", exts: ["zip", "rar", "7z"] },
  { id: "ebook", labelKey: "assocEbook", exts: ["pdf", "epub", "mobi", "azw3", "azw"] },
] as const;

export interface Settings {
  scrollDelta: number;
  scrollTrack: ScrollTrackBehavior;
  smoothScroll: boolean;
  playScrollMode: PlayScrollMode;
  playSmoothStep: number;
  playSmoothInterval: number;
  playIntervalStep: number;
  playIntervalStepMode: PlayIntervalStepMode;
  playIntervalMs: number;
  playAfterScrollPageMs: number;
  playNoScrollPageMs: number;
  autoLoadHistory: boolean;
  /** When true, check GitHub Releases on startup and prompt to download if newer. */
  checkUpdateOnStartup: boolean;
  /** When true, fetch remote data manifest.json on startup. */
  fetchDataManifestOnStartup: boolean;
  syncCurrentFile: boolean;
  readSubfolder: boolean;
  /** When true, PDF pages are rasterized via pdfium; when false, extract embedded images only. */
  pdfRasterize: boolean;
  backgroundColor: string;
  /** Multi-rule filename chapter matching. */
  chapterRules: ChapterFilenameRule[];
  /** Windowed sidebar width (px); persisted. */
  sidebarWidth: number;
  /** Windowed sidebar open/closed; persisted. Float reveal in overlay is separate. */
  sidebarOpen: boolean;
  historyCount: number;
  readingMode: ReadingMode;
  isolateCover: boolean;
  spreadGap: number;
  stagePadX: number;
  stagePadY: number;
  spreadAlign: SpreadAlign;
  minimalistShowSeek: boolean;
  minimalistShowPage: boolean;
  fullscreenShowSeek: boolean;
  fullscreenShowPage: boolean;
  fullscreenShowTime: boolean;
  thumbShowIndex: boolean;
  thumbShowName: boolean;
  thumbSize: number;
  modeFits: ModeFits;
  zoom: number;
  minZoom: number;
  maxZoom: number;
  theme: ThemeName;
  /** Windowed chrome overlay (hide titlebar/toolbar like fullscreen). */
  minimalist: boolean;
  contextMenu: boolean;
  fileAssociations: string[];
  /** Image render / enhance profile (classic, realtime, or neural:assetId). */
  renderProfile: RenderProfile;
  /** CSS filter percents; 100 = neutral. */
  brightness: number;
  contrast: number;
  saturation: number;
  /** Overrides only; missing keys use DEFAULT_SHORTCUTS. */
  shortcuts: Partial<Record<ShortcutActionId, string>>;
  translation: TranslationSettings;
  /**
   * Display-only resolved data root. Never persist a non-empty value — custom
   * paths live in the Rust locator; empty means “use launch default”.
   */
  cacheDir: string;
  /** Long-edge pixel target for neural upscale (also the skip threshold). */
  neuralTargetSize: number;
  proxyEnabled: boolean;
  proxyType: "http" | "socks5" | "socks4";
  proxyHost: string;
  proxyPort: number;
  proxyUser: string;
  proxyPass: string;
}

export const ARCHIVE_EXTS = [
  "zip",
  "cbz",
  "rar",
  "cbr",
  "7z",
  "epub",
  "pdf",
  "mobi",
  "azw3",
  "azw",
] as const;

export type ThemeName = "dark" | "light";

export type FitMode = "auto" | "window" | "original" | "fixedWidth" | "originalScale";

export function normalizeFitMode(value: unknown): FitMode {
  if (
    value === "auto"
    || value === "window"
    || value === "original"
    || value === "fixedWidth"
    || value === "originalScale"
  ) {
    return value;
  }
  return "window";
}

export function normalizeFitFixedWidth(value: unknown): number {
  const number = typeof value === "number" ? value : Number(value);
  if (!Number.isFinite(number)) return 1000;
  return Math.min(10000, Math.max(100, Math.round(number)));
}

export function normalizeFitOriginalScale(value: unknown): number {
  const number = typeof value === "number" ? value : Number(value);
  if (!Number.isFinite(number)) return 1;
  return Math.min(8, Math.max(0.1, Math.round(number * 100) / 100));
}

const READING_MODES: ReadingMode[] = ["single", "double", "scroll"];

export function normalizeReadingMode(value: unknown): ReadingMode {
  if (value === "single" || value === "double" || value === "scroll") return value;
  return "single";
}

export function normalizeDirection(value: unknown): ReadingDirection {
  return value === "ltr" ? "ltr" : "rtl";
}

export function defaultModeFit(mode: ReadingMode): ModeFitSettings {
  return {
    fitMode: mode === "scroll" ? "auto" : "window",
    fitFixedWidth: 1000,
    fitOriginalScale: 1,
    direction: "rtl",
  };
}

export function defaultModeFits(): ModeFits {
  return {
    single: defaultModeFit("single"),
    double: defaultModeFit("double"),
    scroll: defaultModeFit("scroll"),
  };
}

export function normalizeModeFit(raw: unknown, mode: ReadingMode): ModeFitSettings {
  const base = defaultModeFit(mode);
  const src = raw && typeof raw === "object" ? raw as Record<string, unknown> : {};
  let fitMode = normalizeFitMode(src.fitMode ?? base.fitMode);
  if (mode === "scroll" && fitMode === "window") fitMode = "auto";
  return {
    fitMode,
    fitFixedWidth: normalizeFitFixedWidth(src.fitFixedWidth ?? base.fitFixedWidth),
    fitOriginalScale: normalizeFitOriginalScale(src.fitOriginalScale ?? base.fitOriginalScale),
    direction: normalizeDirection(src.direction ?? base.direction),
  };
}

export function normalizeModeFits(raw: unknown): ModeFits {
  const result = defaultModeFits();
  if (!raw || typeof raw !== "object") return result;
  const bag = raw as Record<string, unknown>;
  for (const mode of READING_MODES) {
    if (Object.prototype.hasOwnProperty.call(bag, mode)) {
      result[mode] = normalizeModeFit(bag[mode], mode);
    }
  }
  return result;
}

export function normalizeFileAssociations(value: unknown): string[] {
  if (!Array.isArray(value)) return [];
  const allowed = new Set<string>(ARCHIVE_EXTS);
  const next: string[] = [];
  for (const item of value) {
    if (typeof item !== "string") continue;
    const ext = item.trim().replace(/^\./, "").toLowerCase();
    if (!allowed.has(ext) || next.includes(ext)) continue;
    next.push(ext);
  }
  return next;
}

export function contextMenuLabel(): string {
  return i18n.global.t("contextMenuOpen");
}

export async function setContextMenuEnabled(enabled: boolean) {
  if (!inTauri()) {
    settings.contextMenu = false;
    return;
  }
  await invoke("set_context_menu", { enabled, label: contextMenuLabel() });
  settings.contextMenu = enabled;
}

export async function applyFileAssociations(extensions: string[]) {
  if (!inTauri()) return;
  const next = normalizeFileAssociations(extensions);
  await invoke("apply_file_associations", {
    extensions: next,
    description: i18n.global.t("appName"),
  });
  settings.fileAssociations = next;
}

export async function refreshShellIntegration() {
  if (!inTauri()) {
    return { contextMenu: false, associations: [] as string[], supported: false };
  }
  return invoke<{
    contextMenu: boolean;
    associations: string[];
    supported: boolean;
  }>("get_shell_integration");
}

interface ExtractProgress {
  sessionId: number;
  done: number;
  total: number;
  finished: boolean;
  error: string | null;
  index?: number | null;
  running?: number | null;
}

interface PageMetric {
  sessionId: number;
  index: number;
  width: number;
  height: number;
  name?: string | null;
  relPath?: string | null;
}

interface PageMetricsBatch {
  sessionId: number;
  pages: PageMetric[];
}

interface MeasureProgress {
  sessionId: number;
  done: number;
  total: number;
  finished: boolean;
}

interface AboutInfo {
  version: string;
  os: string;
  arch: string;
}

function clampInt(value: unknown, min: number, max: number, fallback: number) {
  const number = typeof value === "number" ? value : Number(value);
  if (!Number.isFinite(number)) return fallback;
  return Math.min(max, Math.max(min, Math.round(number)));
}

function normalizeScrollTrack(value: unknown): ScrollTrackBehavior {
  if (value === "off" || value === "page" || value === "jump") return value;
  return "page";
}

function normalizeSpreadAlign(value: unknown): SpreadAlign {
  if (value === "top" || value === "center" || value === "bottom") return value;
  return "top";
}

function normalizePlayScrollMode(value: unknown): PlayScrollMode {
  return value === "interval" ? "interval" : "smooth";
}

function normalizePlayIntervalStepMode(value: unknown): PlayIntervalStepMode {
  return value === "custom" ? "custom" : "screen";
}

const defaultSettings = (): Settings => ({
  scrollDelta: 100,
  scrollTrack: "page",
  smoothScroll: true,
  playScrollMode: "smooth",
  playSmoothStep: 1,
  playSmoothInterval: 16,
  playIntervalStep: 240,
  playIntervalStepMode: "screen",
  playIntervalMs: 2500,
  playAfterScrollPageMs: 800,
  playNoScrollPageMs: 2500,
  autoLoadHistory: true,
  checkUpdateOnStartup: true,
  fetchDataManifestOnStartup: true,
  syncCurrentFile: false,
  readSubfolder: true,
  pdfRasterize: false,
  backgroundColor: "#000000",
  chapterRules: createDefaultChapterRules(),
  sidebarWidth: 200,
  sidebarOpen: false,
  historyCount: 10,
  readingMode: "single",
  isolateCover: false,
  spreadGap: 0,
  stagePadX: 0,
  stagePadY: 0,
  spreadAlign: "top",
  minimalistShowSeek: true,
  minimalistShowPage: false,
  fullscreenShowSeek: true,
  fullscreenShowPage: true,
  fullscreenShowTime: true,
  thumbShowIndex: true,
  thumbShowName: true,
  thumbSize: 128,
  modeFits: defaultModeFits(),
  zoom: 1,
  minZoom: 0.25,
  maxZoom: 4,
  theme: "dark",
  minimalist: false,
  contextMenu: false,
  fileAssociations: [],
  renderProfile: "bicubic",
  brightness: 100,
  contrast: 100,
  saturation: 100,
  shortcuts: {},
  translation: defaultTranslationSettings(),
  cacheDir: "",
  neuralTargetSize: 2048,
  proxyEnabled: false,
  proxyType: "http",
  proxyHost: "127.0.0.1",
  proxyPort: 7890,
  proxyUser: "",
  proxyPass: "",
});

export function normalizeColorPercent(value: unknown, fallback = 100): number {
  const number = typeof value === "number" ? value : Number(value);
  if (!Number.isFinite(number)) return fallback;
  return Math.min(200, Math.max(0, Math.round(number)));
}

const canvasColor = { dark: "#000000", light: "#ffffff" } as const;
const themeBackgrounds = new Set(["#000000", "#ffffff", "#f3f3f3"]);

function alignCanvas() {
  const theme = settings.theme === "light" ? "light" : "dark";
  const background = settings.backgroundColor.trim().toLowerCase();
  if (themeBackgrounds.has(background)) {
    settings.backgroundColor = canvasColor[theme];
  }
}

const initialSettings = defaultSettings();
if (!inTauri()) {
  try {
    const saved = JSON.parse(localStorage.getItem("supermanga") || "{}").settings;
    if (saved?.theme === "light") {
      initialSettings.theme = "light";
      const background = String(saved.backgroundColor ?? "").trim().toLowerCase();
      if (!background || themeBackgrounds.has(background)) initialSettings.backgroundColor = canvasColor.light;
    }
  } catch {
    // Keep the dark default when stored settings cannot be read.
  }
}

export const settings = reactive<Settings>(initialSettings);

export function modeFit(mode: ReadingMode = settings.readingMode): ModeFitSettings {
  return settings.modeFits[mode];
}

function normalizeShortcutOverrides(raw: unknown): Partial<Record<ShortcutActionId, string>> {
  const merged = mergeShortcutBindings(
    DEFAULT_SHORTCUTS,
    raw && typeof raw === "object" ? (raw as Partial<Record<string, string>>) : undefined,
  );
  return shortcutOverrides(DEFAULT_SHORTCUTS, merged);
}

function applySettings(saved: Partial<Settings>) {
  Object.assign(settings, defaultSettings(), saved);
  settings.readingMode = normalizeReadingMode(settings.readingMode);
  settings.modeFits = normalizeModeFits(saved.modeFits);
  settings.renderProfile = normalizeRenderProfile(settings.renderProfile);
  settings.brightness = normalizeColorPercent(settings.brightness);
  settings.contrast = normalizeColorPercent(settings.contrast);
  settings.saturation = normalizeColorPercent(settings.saturation);
  settings.shortcuts = normalizeShortcutOverrides(saved.shortcuts);
  settings.translation = mergeTranslationSettings(saved.translation);
  settings.cacheDir = typeof saved.cacheDir === "string" ? saved.cacheDir : "";
  {
    const target = Math.round(Number(saved.neuralTargetSize));
    settings.neuralTargetSize =
      Number.isFinite(target) && target > 0
        ? Math.min(8192, Math.max(512, target))
        : 2048;
  }
  settings.proxyEnabled = !!saved.proxyEnabled;
  settings.proxyType =
    saved.proxyType === "socks5" || saved.proxyType === "socks4"
      ? saved.proxyType
      : "http";
  settings.proxyHost = typeof saved.proxyHost === "string" ? saved.proxyHost : "127.0.0.1";
  const port = Number(saved.proxyPort);
  settings.proxyPort = Number.isFinite(port) && port > 0 ? Math.round(port) : 7890;
  settings.proxyUser = typeof saved.proxyUser === "string" ? saved.proxyUser : "";
  settings.proxyPass = typeof saved.proxyPass === "string" ? saved.proxyPass : "";
  settings.minimalist = !!settings.minimalist;
  settings.syncCurrentFile = !!saved.syncCurrentFile;
  settings.checkUpdateOnStartup = saved.checkUpdateOnStartup !== false;
  settings.fetchDataManifestOnStartup = saved.fetchDataManifestOnStartup !== false;
  settings.pdfRasterize = saved.pdfRasterize === true;
  settings.scrollTrack = normalizeScrollTrack(saved.scrollTrack);
  settings.scrollDelta = clampInt(saved.scrollDelta, 1, 400, 100);
  settings.smoothScroll = saved.smoothScroll !== false;
  settings.playScrollMode = normalizePlayScrollMode(saved.playScrollMode);
  settings.playSmoothStep = clampInt(saved.playSmoothStep, 1, 400, 1);
  settings.playSmoothInterval = clampInt(saved.playSmoothInterval, 1, 1000, 16);
  settings.playIntervalStep = clampInt(saved.playIntervalStep, 1, 2000, 240);
  settings.playIntervalStepMode = normalizePlayIntervalStepMode(saved.playIntervalStepMode);
  settings.playIntervalMs = clampInt(saved.playIntervalMs, 1, 10000, 2500);
  settings.playAfterScrollPageMs = clampInt(saved.playAfterScrollPageMs, 0, 60000, 800);
  settings.playNoScrollPageMs = clampInt(saved.playNoScrollPageMs, 1, 60000, 2500);
  settings.historyCount = clampInt(saved.historyCount, 0, 99, 10);
  settings.spreadGap = clampInt(saved.spreadGap, 0, 200, 0);
  settings.stagePadX = clampInt(saved.stagePadX, 0, 400, 0);
  settings.stagePadY = clampInt(saved.stagePadY, 0, 400, 0);
  settings.spreadAlign = normalizeSpreadAlign(saved.spreadAlign);
  settings.isolateCover = !!saved.isolateCover;
  settings.minimalistShowSeek = saved.minimalistShowSeek !== false;
  settings.minimalistShowPage = !!saved.minimalistShowPage;
  settings.fullscreenShowSeek = saved.fullscreenShowSeek !== false;
  settings.fullscreenShowPage = saved.fullscreenShowPage !== false;
  settings.fullscreenShowTime = saved.fullscreenShowTime !== false;
  settings.thumbShowIndex = saved.thumbShowIndex !== false;
  settings.thumbShowName = saved.thumbShowName !== false;
  settings.thumbSize = clampInt(saved.thumbSize, THUMB_SIZE_MIN, THUMB_SIZE_MAX, 128);
  settings.chapterRules = resolveChapterRules(saved.chapterRules);
  settings.sidebarWidth = clampInt(saved.sidebarWidth, 200, 2000, 200);
  settings.sidebarOpen = !!saved.sidebarOpen;
}

export const THUMB_SIZE_MIN = 72;
export const THUMB_SIZE_MAX = 320;

export function scrollStepPx() {
  return Math.max(1, settings.scrollDelta);
}

export function downloadNetworkConfig() {
  return {
    proxyEnabled: settings.proxyEnabled,
    proxyType: settings.proxyType,
    proxyHost: settings.proxyHost,
    proxyPort: settings.proxyPort,
    proxyUser: settings.proxyUser,
    proxyPass: settings.proxyPass,
  };
}

export const shortcutBindings = computed<ShortcutBindingMap>(() =>
  mergeShortcutBindings(DEFAULT_SHORTCUTS, settings.shortcuts),
);

export function shortcutDisplay(action: ShortcutActionId): string {
  return acceleratorToDisplayText(shortcutBindings.value[action]);
}

export function tipWithShortcut(label: string, action: ShortcutActionId): string {
  return titleWithShortcut(label, shortcutBindings.value[action]);
}

function isFixedAccelerator(accel: string): boolean {
  const normalized = normalizeAccelerator(accel);
  if (!normalized) return false;
  return (
    normalized === normalizeAccelerator(FIXED_DEVTOOLS_ACCEL)
    || normalized === normalizeAccelerator(FIXED_LOGS_ACCEL)
    || normalized === "Alt"
  );
}

/** True when `accel` is used by another action or a fixed chord (except `action`). */
export function isShortcutAccelOccupied(accel: string, action?: ShortcutActionId): boolean {
  const normalized = normalizeAccelerator(accel);
  if (!normalized || normalized === "Escape") return false;
  if (isFixedAccelerator(normalized)) return true;
  for (const id of Object.keys(shortcutBindings.value) as ShortcutActionId[]) {
    if (action && id === action) continue;
    if (normalizeAccelerator(shortcutBindings.value[id]) === normalized) return true;
  }
  return false;
}

export function setShortcutBinding(action: ShortcutActionId, accel: string): boolean {
  if (!isListedShortcut(action)) return false;
  const normalized = normalizeAccelerator(accel);
  if (!normalized || normalized === "Escape") return false;
  if (isShortcutAccelOccupied(normalized, action)) return false;
  const next = { ...shortcutBindings.value, [action]: normalized };
  settings.shortcuts = shortcutOverrides(DEFAULT_SHORTCUTS, next);
  return true;
}

export function resetShortcutBindings() {
  settings.shortcuts = {};
}

export function setRenderProfile(profile: RenderProfile) {
  settings.renderProfile = normalizeRenderProfile(profile);
}

function scheduleNeuralBook() {
  const book = comic.value;
  void syncNeuralBook({
    enabled: !!book && isNeuralProfile(settings.renderProfile),
    inTauri: inTauri(),
    sessionId: book?.sessionId ?? 0,
    path: book?.path ?? "",
    readSubfolder: settings.readSubfolder,
    pages: book?.pages ?? [],
    focusIndex: pageIndex.value,
    readingMode: settings.readingMode,
    isolateCover: settings.isolateCover,
    renderProfile: settings.renderProfile,
    targetSize: settings.neuralTargetSize,
    imageSrc,
  });
}

/** Page-index-only: retarget queue priorities without re-probing the whole book on disk. */
function scheduleNeuralFocus() {
  const book = comic.value;
  if (!book || !isNeuralProfile(settings.renderProfile) || !inTauri()) return;
  setNeuralFocus(pageIndex.value, book.pages.length, {
    readingMode: settings.readingMode,
    isolateCover: settings.isolateCover,
  });
}

export function openColorAdjust() {
  colorAdjustOpen.value = true;
}

export function resetColorAdjust() {
  settings.brightness = 100;
  settings.contrast = 100;
  settings.saturation = 100;
}

/** CSS filter for page/scroll surfaces; identity when all at 100. */
export function pageColorFilter(): string {
  const b = normalizeColorPercent(settings.brightness) / 100;
  const c = normalizeColorPercent(settings.contrast) / 100;
  const s = normalizeColorPercent(settings.saturation) / 100;
  if (b === 1 && c === 1 && s === 1) return "none";
  return `brightness(${b}) contrast(${c}) saturate(${s})`;
}

watch(() => settings.theme, (theme) => {
  const next = theme === "light" ? "light" : "dark";
  if (theme !== next) {
    settings.theme = next;
    return;
  }
  document.documentElement.classList.toggle("theme-light", next === "light");
  document.documentElement.style.colorScheme = next;
  // Sync native window chrome (title bar).
  if (inTauri()) {
    void getCurrentWindow().setTheme(next).catch(() => {});
    void import("./lib/appLog").then(({ broadcastUiSync }) => broadcastUiSync({ theme: next }));
  }
  const background = settings.backgroundColor.trim().toLowerCase();
  if (themeBackgrounds.has(background)) {
    settings.backgroundColor = canvasColor[next];
  }
}, { immediate: true });

export function toggleTheme() {
  const next = settings.theme === "light" ? "dark" : "light";
  settings.theme = next;
  settings.backgroundColor = canvasColor[next];
}

export function toggleMinimalist() {
  settings.minimalist = !settings.minimalist;
}
export const history = ref<HistoryItem[]>([]);
/** When false, startup skips the most recent file. Closing a file clears this; opening one sets it. */
let resumeSession = true;
export const comic = ref<Comic | null>(null);
export const pageIndex = ref(0);
export type PageRotation = 0 | 90 | 180 | 270;
/** Cumulative spin degrees (may exceed 360) so clockwise steps can animate forward. */
export const pageSpins = reactive<Record<number, number>>({});
export const rotateGuide = ref(false);
export const scrollOffset = ref(0);

function normalizeRotation(degrees: number): PageRotation {
  const deg = ((degrees % 360) + 360) % 360;
  if (deg === 90 || deg === 180 || deg === 270) return deg;
  return 0;
}

export function getPageSpin(index: number) {
  return pageSpins[index] ?? 0;
}

export function getPageRotation(index: number): PageRotation {
  return normalizeRotation(getPageSpin(index));
}

export function setPageRotation(index: number, rotation: PageRotation) {
  const current = getPageSpin(index);
  const logical = normalizeRotation(current);
  let delta = rotation - logical;
  if (delta > 180) delta -= 360;
  if (delta < -180) delta += 360;
  const next = current + delta;
  if (next === 0) {
    if (index in pageSpins) delete pageSpins[index];
    return;
  }
  pageSpins[index] = next;
}

export function rotatePageClockwise(index = pageIndex.value) {
  pageSpins[index] = getPageSpin(index) + 90;
}

export function rotatePageCounterClockwise(index = pageIndex.value) {
  pageSpins[index] = getPageSpin(index) - 90;
}

export function clearPageRotations() {
  for (const key of Object.keys(pageSpins)) delete pageSpins[Number(key)];
}

export function rotatedSize(width: number, height: number, rotation: PageRotation) {
  if (rotation === 90 || rotation === 270) return { width: height, height: width };
  return { width, height };
}

export function rotationSwapped(rotation: PageRotation) {
  return rotation === 90 || rotation === 270;
}
export const zoom = computed(() => settings.zoom);
export type SettingsTabId =
  | "general"
  | "reading"
  | "neural"
  | "translate"
  | "proxy"
  | "system";

export const settingsOpen = ref(false);
export const settingsActiveTab = ref<SettingsTabId>("general");
export const shortcutsOpen = ref(false);
export const ocrAssetsOpen = ref(false);
export const detectorAssetsOpen = ref(false);
export const neuralAssetsOpen = ref(false);
export const ocrEnabled = ref(false);
export const ocrBusy = ref(false);
/** Installed OCR / detector pack ids (for enable-translation gating). */
export const installedOcrIds = ref<string[]>([]);
/** Shared ORT runtime ready (local ONNX OCR packs + detectors). */
export const ortRuntimeReady = ref(false);
/** Full-screen blocking overlay (e.g. neural cache migrate). */
export const globalBusy = ref(false);
export const globalBusyText = ref("");
export const ocrRegionsByPage = reactive<Record<number, OcrRegion[]>>({});
export const colorAdjustOpen = ref(false);

/** True while the thumbnail-size slider is being scrubbed (pointer/keyboard). */
export const thumbSizeScrubbing = ref(false);

export function beginThumbSizeGesture() {
  thumbSizeScrubbing.value = true;
}

export function endThumbSizeGesture() {
  thumbSizeScrubbing.value = false;
}

export function openOcrAssets() {
  ocrAssetsOpen.value = true;
}

export function openSettings(tab?: SettingsTabId) {
  if (tab) settingsActiveTab.value = tab;
  settingsOpen.value = true;
}

export function openImageAssets() {
  openSettings("neural");
}

export function openProxySettings() {
  openSettings("proxy");
}

export function openTranslationSettings() {
  openSettings("translate");
}

export function clearOcrRegions() {
  for (const key of Object.keys(ocrRegionsByPage)) delete ocrRegionsByPage[Number(key)];
}

function pageHasOcrResult(index: number) {
  return Object.prototype.hasOwnProperty.call(ocrRegionsByPage, index);
}

/** Scroll-mode pages intersecting the viewport (partial counts). Cleared when leaving scroll. */
export const scrollViewportPages = ref<number[]>([]);

/**
 * When false, scroll-mode heavy work should wait: classic resample, Anime4K,
 * neural upscale kicks, and OCR/translate. ScrollView clears this on motion
 * and sets it true after settle.
 */
export const scrollResampleReady = ref(true);
const SCROLL_RESAMPLE_SETTLE_MS = 140;
let scrollResampleTimer = 0;

/** Call on scroll / glide activity so expensive work waits until motion stops. */
export function markScrollMotion() {
  if (settings.readingMode !== "scroll") return;
  if (scrollResampleReady.value) scrollResampleReady.value = false;
  window.clearTimeout(scrollResampleTimer);
  scrollResampleTimer = window.setTimeout(() => {
    scrollResampleReady.value = true;
    // Decide cancel vs keep only after settle (small nudge may not change page).
    resyncTranslateKeepingInflight();
  }, SCROLL_RESAMPLE_SETTLE_MS);
}

let ocrEnsureGen = 0;
let ocrEnsureRunning = false;
/** Re-scan targets after the current in-flight page finishes (do not cancel it). */
let ocrEnsureQueued = false;
/** Page index currently inside OCR/translate; null when idle. */
export const ocrRunningIndex = ref<number | null>(null);

function ocrPagesEqual(a: number[], b: number[] | undefined) {
  if (!b || a.length !== b.length) return false;
  return a.every((page, i) => page === b[i]);
}

/** Drop in-flight OCR/translate so a page turn can start the new page immediately. */
function cancelInFlightOcr() {
  ocrEnsureGen += 1;
  ocrEnsureRunning = false;
  ocrEnsureQueued = false;
  ocrRunningIndex.value = null;
  ocrBusy.value = false;
  console.info("[ocr]", "cancel");
  if (inTauri()) {
    void invoke("cancel_ocr_translate").catch(() => {});
  }
}

/** Build the next OCR/translate work list (priority-ranked; batch only when book pre-translate is on). */
function collectTranslateQueue(): number[] {
  const count = pageCount.value;
  if (count <= 0) return [];
  const view =
    translationTargetPages.value.length > 0
      ? translationTargetPages.value
      : [pageIndex.value];
  const near = nearFocusTranslatePages(
    pageIndex.value,
    count,
    settings.readingMode,
    settings.isolateCover,
  );
  // Off: current view + neighbor spreads (same idea as neural when book pre-upscale is off).
  // On: whole book, ranked by priority.
  const candidates = settings.translation.bookPreTranslate
    ? Array.from({ length: count }, (_, index) => index)
    : [...new Set([...view, ...near])];
  const pending = candidates.filter((index) => !pageHasOcrResult(index));
  const ranked = rankTranslatePages(
    pending,
    pageIndex.value,
    count,
    settings.readingMode,
    settings.isolateCover,
  );
  // Scroll viewport pages may span more than focus±1; keep them ahead of batch.
  if (settings.readingMode === "scroll") {
    const inView = new Set(view);
    return [...ranked.filter((i) => inView.has(i)), ...ranked.filter((i) => !inView.has(i))];
  }
  return ranked;
}

/** Live set that should not cancel an in-flight page when still overlapping. */
function translateKeepSet(): number[] {
  const count = pageCount.value;
  if (settings.translation.bookPreTranslate) {
    const pending: number[] = [];
    for (let index = 0; index < count; index += 1) {
      if (!pageHasOcrResult(index) || ocrRunningIndex.value === index) pending.push(index);
    }
    return pending;
  }
  const view =
    translationTargetPages.value.length > 0
      ? translationTargetPages.value
      : [pageIndex.value];
  const near = nearFocusTranslatePages(
    pageIndex.value,
    count,
    settings.readingMode,
    settings.isolateCover,
  );
  return [...new Set([...view, ...near])];
}

/**
 * After scroll settle (or a non-scroll target change): cancel in-flight OCR only
 * when it left the keep set; otherwise let it finish and queue a resync.
 */
function resyncTranslateKeepingInflight() {
  if (!ocrEnabled.value) return;
  if (ocrBusy.value || ocrEnsureRunning) {
    const inflight = ocrRunningIndex.value;
    const keep = translateKeepSet();
    if (inflight != null && keep.includes(inflight)) {
      ocrEnsureQueued = true;
      return;
    }
    // Book pre-translate: never cancel a still-needed page on viewport churn.
    if (settings.translation.bookPreTranslate && inflight != null && !pageHasOcrResult(inflight)) {
      ocrEnsureQueued = true;
      return;
    }
    cancelInFlightOcr();
  }
  void ensureShownPagesTranslated();
}

/** Target / focus changed: cancel only if the in-flight page left the keep set. */
function onTranslationTargetsChanged(pages: number[], prev: number[] | undefined) {
  if (!ocrEnabled.value) return;
  if (ocrPagesEqual(pages, prev)) return;
  // Viewport churn during scroll: one pass after settle (see markScrollMotion).
  if (settings.readingMode === "scroll" && !scrollResampleReady.value) {
    ocrEnsureQueued = true;
    return;
  }
  resyncTranslateKeepingInflight();
}

function scheduleTranslateBookResync() {
  if (!ocrEnabled.value) return;
  if (settings.readingMode === "scroll" && !scrollResampleReady.value) {
    ocrEnsureQueued = true;
    return;
  }
  if (ocrBusy.value || ocrEnsureRunning) {
    ocrEnsureQueued = true;
    return;
  }
  void ensureShownPagesTranslated();
}

export function ocrReadinessContext(): OcrReadinessContext {
  return {
    installedAssetIds: installedOcrIds.value,
    runtimeReady: ortRuntimeReady.value,
  };
}

/** True when at least one OCR backend can run (platform + packs + runtime). */
export function canEnableTranslation(): boolean {
  if (!inTauri()) return true;
  return hasReadyOcrEngine(ocrReadinessContext());
}

export async function refreshTranslationPrereqs() {
  if (!inTauri()) {
    ortRuntimeReady.value = true;
    installedOcrIds.value = [];
    return;
  }
  try {
    const [status, ids] = await Promise.all([
      neuralRuntimeStatus(),
      installedOcrAssetIds(),
    ]);
    ortRuntimeReady.value = !!status.ready;
    installedOcrIds.value = ids;
  } catch {
    ortRuntimeReady.value = false;
  }
  const engine = normalizeOcrEngineId(settings.translation.ocrEngine);
  if (engine !== settings.translation.ocrEngine) {
    settings.translation.ocrEngine = engine;
  }
  if (ocrEnabled.value && !canEnableTranslation()) {
    ocrEnabled.value = false;
    cancelInFlightOcr();
  }
}

/** Toggle translation mode (overlay + auto OCR for the current view). */
export async function toggleTranslationMode() {
  if (!comic.value) return;
  if (ocrEnabled.value) {
    // Cancel in-flight OCR/translate.
    ocrEnabled.value = false;
    cancelInFlightOcr();
    return;
  }
  await refreshTranslationPrereqs();
  if (!canEnableTranslation()) {
    notifyWarning(i18n.global.t("translationUnavailable"));
    return;
  }
  const picked = pickReadyOcrEngine(
    normalizeOcrEngineId(settings.translation.ocrEngine),
    ocrReadinessContext(),
  );
  if (!picked) {
    notifyWarning(i18n.global.t("translationUnavailable"));
    return;
  }
  if (picked !== settings.translation.ocrEngine) {
    settings.translation.ocrEngine = picked;
  }
  ocrEnabled.value = true;
  // Fresh pass so geometry fixes / cache bumps are applied.
  clearOcrRegions();
  await ensureShownPagesTranslated();
}

/** Force re-OCR + translate the current page (keeps translation mode on). */
export async function retranslateCurrentPage(index = pageIndex.value) {
  if (!comic.value) return;
  await refreshTranslationPrereqs();
  if (!canEnableTranslation()) {
    notifyWarning(i18n.global.t("translationUnavailable"));
    return;
  }
  const picked = pickReadyOcrEngine(
    normalizeOcrEngineId(settings.translation.ocrEngine),
    ocrReadinessContext(),
  );
  if (!picked) {
    notifyWarning(i18n.global.t("translationUnavailable"));
    return;
  }
  if (picked !== settings.translation.ocrEngine) {
    settings.translation.ocrEngine = picked;
  }
  ocrEnabled.value = true;
  if (ocrBusy.value || ocrEnsureRunning) {
    cancelInFlightOcr();
  }
  delete ocrRegionsByPage[index];
  await runOcrTranslate(index, { quietEmpty: false, forceRefresh: true });
}

/** OCR + translate for the current view, and the rest of the book when pre-translate is on. */
export async function ensureShownPagesTranslated() {
  if (!ocrEnabled.value || !comic.value) return;
  if (settings.readingMode === "scroll" && !scrollResampleReady.value) {
    ocrEnsureQueued = true;
    return;
  }
  if (ocrEnsureRunning) {
    ocrEnsureQueued = true;
    return;
  }
  const gen = ocrEnsureGen;
  ocrEnsureRunning = true;
  ocrBusy.value = true;
  try {
    do {
      ocrEnsureQueued = false;
      if (!ocrEnabled.value || gen !== ocrEnsureGen) return;
      const pages = collectTranslateQueue();
      for (const index of pages) {
        if (!ocrEnabled.value || gen !== ocrEnsureGen) return;
        if (pageHasOcrResult(index)) continue;
        // Without book pre-translate, drop pages that left current+neighbor set mid-loop.
        if (!settings.translation.bookPreTranslate && !translateKeepSet().includes(index)) {
          continue;
        }
        await runOcrTranslateInner(index, { quietEmpty: true }, gen);
        if (gen !== ocrEnsureGen) return;
        // Re-rank after each page when book mode may have shifted focus.
        if (ocrEnsureQueued) break;
      }
    } while (ocrEnsureQueued && gen === ocrEnsureGen && ocrEnabled.value);
  } finally {
    if (gen === ocrEnsureGen) {
      ocrEnsureRunning = false;
      ocrBusy.value = false;
      ocrRunningIndex.value = null;
      if (ocrEnsureQueued && ocrEnabled.value) {
        ocrEnsureQueued = false;
        void ensureShownPagesTranslated();
      }
    }
  }
}

type RunOcrOptions = {
  /** Skip empty-result toast (used by auto translate). */
  quietEmpty?: boolean;
  /** Bypass disk OCR cache and re-run recognition. */
  forceRefresh?: boolean;
};

/** Core OCR + translate for one page (caller owns ocrBusy / ensure lifecycle). */
async function runOcrTranslateInner(
  index: number,
  options: RunOcrOptions,
  gen: number,
): Promise<void> {
  const current = comic.value;
  if (!current) return;
  const sourceLang = settings.translation.sourceLang || "auto";
  const ocrEngine = normalizeOcrEngineId(settings.translation.ocrEngine);
  // Skip no-op writes to avoid re-entering the OCR clear watch.
  const required = requiredOcrAssetIdsForEngine(ocrEngine);
  if (required.length) {
    const installed = new Set(await installedOcrAssetIds());
    const missing = required.filter((id) => !installed.has(id));
    if (missing.length) {
      notifyWarning(i18n.global.t("ocrModelsRequired"));
      ocrAssetsOpen.value = true;
      // Mark attempted so auto-translate does not spam.
      ocrRegionsByPage[index] = [];
      return;
    }
  }
  if (gen !== ocrEnsureGen || !ocrEnabled.value) return;
  ocrEnabled.value = true;
  ocrRunningIndex.value = index;
  try {
    if (!inTauri()) {
      if (gen !== ocrEnsureGen || !ocrEnabled.value) return;
      ocrRegionsByPage[index] = [
        {
          id: "demo-1",
          x: 0.12,
          y: 0.18,
          w: 0.28,
          h: 0.12,
          text: "デモ",
          translation: "演示",
        },
      ];
      return;
    }
    const result = await invoke<OcrPageResult>("ocr_translate_page", {
      args: {
        sessionId: current.sessionId,
        index,
        sourceLang,
        ocrEngine,
        settings: settings.translation,
        forceRefresh: !!options.forceRefresh,
      },
    });
    if (gen !== ocrEnsureGen || !ocrEnabled.value) return;
    const detectorParts = [result.textDetector || settings.translation.textDetector || "off"];
    if (result.auxYolo ?? settings.translation.enableAuxYolo) detectorParts.push("aux-yolo");
    if (result.saberYoloRefine ?? settings.translation.enableSaberYoloRefine) {
      detectorParts.push("saber-yolo-refine");
    }
    const priority = translatePriorityForIndex(
      index,
      pageIndex.value,
      pageCount.value,
      settings.readingMode,
      settings.isolateCover,
    );
    console.info(
      "[ocr]",
      JSON.stringify({
        engine: result.engine,
        textDetector: detectorParts.join("+"),
        sourceLang: result.sourceLang || sourceLang,
        targetLang: result.targetLang || settings.translation.targetLang,
        translationProvider: result.translationProvider || settings.translation.provider,
        imagePath: result.imagePath || "",
        cached: result.cached,
        regionCount: result.regions.length,
        priority: priority >= TRANSLATE_PRIORITY_BATCH ? "batch" : priority,
        regions: result.regions,
      }),
    );
    ocrRegionsByPage[index] = result.regions;
    if (result.translationError) {
      notifyWarning(formatTranslationError(result.translationError));
    } else if (!result.regions.length && !options.quietEmpty) {
      notifyInfo(i18n.global.t("ocrEmpty"));
    }
  } catch (error) {
    if (gen !== ocrEnsureGen || !ocrEnabled.value) return;
    const message = String(error);
    // Page turn / mode off — do not toast or mark the page as empty.
    if (message.includes("OCR_CANCELLED") || /cancelled/i.test(message)) {
      console.info("[ocr]", "cancelled");
      return;
    }
    // Record empty result so auto-translate / watchers do not retry forever.
    ocrRegionsByPage[index] = [];
    if (message.includes("OCR_MODELS_REQUIRED")) {
      notifyWarning(i18n.global.t("ocrModelsRequired"));
      ocrAssetsOpen.value = true;
    } else if (isTranslationErrorCode(message)) {
      notifyError(formatTranslationError(message));
    } else {
      notifyError(message);
    }
  } finally {
    if (ocrRunningIndex.value === index) ocrRunningIndex.value = null;
  }
}

/** OCR + translate for one page. Enables translation mode unless cancelled by preflight. */
export async function runOcrTranslate(index = pageIndex.value, options: RunOcrOptions = {}) {
  const current = comic.value;
  if (!current) return;
  // Another page's job still marked busy → treat as superseded (caller should cancel first).
  if (ocrBusy.value) return;
  const gen = ocrEnsureGen;
  ocrBusy.value = true;
  try {
    await runOcrTranslateInner(index, options, gen);
  } finally {
    if (gen === ocrEnsureGen) ocrBusy.value = false;
  }
}
export const aboutOpen = ref(false);
export const runtimeRestartOpen = ref(false);
export const imageInfoOpen = ref(false);
export const chapterRuleOpen = ref(false);
export const chapterRuleError = ref("");
export const sideOpen = toRef(settings, "sidebarOpen");
export const sideTab = ref<"bookmarks" | "chapters">("bookmarks");
export const bookmarks = ref<Bookmark[]>([]);
export const showFullscreenSidebar = ref(false);
export const resizingSidebar = ref(false);
/** Fullscreen-only drag width; cleared when leaving overlay. Not persisted. */
export const fullscreenSidebarWidth = ref<number | null>(null);

export const SIDEBAR_MIN_WIDTH = 200;
export const SIDEBAR_MIN_READER_WIDTH = 300;
export const FULLSCREEN_LEFT_EDGE_PX = 20;

/** Floating top / bottom chrome in fullscreen / minimalist. */
export const chromeTop = ref(false);
export const chromeBottom = ref(false);

export const sidebarWidthForLayout = computed(() => {
  if (chromeOverlay.value && fullscreenSidebarWidth.value != null) {
    return fullscreenSidebarWidth.value;
  }
  return settings.sidebarWidth;
});

export function clampSidebarWidth(width: number): number {
  const max = Math.max(
    SIDEBAR_MIN_WIDTH,
    window.innerWidth - SIDEBAR_MIN_READER_WIDTH,
  );
  return Math.min(max, Math.max(SIDEBAR_MIN_WIDTH, Math.round(width)));
}

export function startResizeSidebar(ev: MouseEvent) {
  ev.preventDefault();
  ev.stopPropagation();
  resizingSidebar.value = true;
}

export function endSidebarResize() {
  if (!resizingSidebar.value) return;
  resizingSidebar.value = false;
  // Only windowed width is persisted.
  settings.sidebarWidth = clampSidebarWidth(settings.sidebarWidth);
}

export function applySidebarResize(clientX: number) {
  if (!resizingSidebar.value) return;
  const next = clampSidebarWidth(clientX);
  if (chromeOverlay.value) fullscreenSidebarWidth.value = next;
  else settings.sidebarWidth = next;
}

/** Edge-hover mutex: only one floating chrome panel via edges at a time. */
export function canShowFullscreenPanel(
  which: "header" | "sidebar" | "footer",
): boolean {
  if (which === "header") {
    return !showFullscreenSidebar.value && !chromeBottom.value;
  }
  if (which === "sidebar") {
    return !chromeTop.value && !chromeBottom.value;
  }
  return !chromeTop.value && !showFullscreenSidebar.value;
}

export function chromeEdgeRevealBlockedByOverlay(): boolean {
  return maskModalOpen.value || globalBusy.value;
}

export function collapseAllChromeForBlockingOverlay() {
  if (!chromeOverlay.value) return;
  chromeTop.value = false;
  chromeBottom.value = false;
  showFullscreenSidebar.value = false;
}
export function revealFullscreenSidebar() {
  if (!chromeOverlay.value || !comic.value) return;
  showFullscreenSidebar.value = true;
}

export function hideFullscreenSidebar() {
  showFullscreenSidebar.value = false;
}

export function clampSidebarWidthToViewport() {
  settings.sidebarWidth = clampSidebarWidth(settings.sidebarWidth);
  if (fullscreenSidebarWidth.value != null) {
    fullscreenSidebarWidth.value = clampSidebarWidth(fullscreenSidebarWidth.value);
  }
}
export const bookmarkDialog = reactive({
  open: false,
  page: 1,
  comment: "",
  editIndex: null as number | null,
});
export const opening = ref(false);
export const showEmpty = ref(false);

const PDF_PASSWORD_REQUIRED = "pdf_password_required";
const PDF_PASSWORD_INCORRECT = "pdf_password_incorrect";

export type PdfPasswordPromptState = {
  wrong: boolean;
  resolve: (value: string | null) => void;
};

export const pdfPasswordPrompt = ref<PdfPasswordPromptState | null>(null);

export function resolvePdfPasswordPrompt(value: string | null) {
  const prompt = pdfPasswordPrompt.value;
  if (!prompt) return;
  pdfPasswordPrompt.value = null;
  prompt.resolve(value);
}

function askPdfPassword(wrong: boolean): Promise<string | null> {
  return new Promise((resolve) => {
    const previous = pdfPasswordPrompt.value;
    if (previous) previous.resolve(null);
    pdfPasswordPrompt.value = { wrong, resolve };
  });
}

function isPdfPasswordError(message: string) {
  return message === PDF_PASSWORD_REQUIRED || message === PDF_PASSWORD_INCORRECT;
}
export const extracting = ref(false);
export const extractDone = ref(0);
export const extractTotal = ref(0);
/** Pages written during extract/rasterize (out-of-order capable). */
export const extractReadyFlags = ref<boolean[]>([]);
export const extractRunningIndex = ref<number | null>(null);
let extractPriorityTimer = 0;
export const measuring = ref(false);
export const measureDone = ref(0);
export const measureTotal = ref(0);
export const fullscreen = ref(false);
export const chromeOverlay = computed(() => fullscreen.value || settings.minimalist);
/** Fullscreen / minimalist chrome hover strip (top/bottom). */
export const FULLSCREEN_CHROME_ZONE = 48;
export const FULLSCREEN_RIGHT_GUTTER = 20;

watch(chromeOverlay, (overlay) => {
  if (overlay) {
    fullscreenSidebarWidth.value = clampSidebarWidth(settings.sidebarWidth);
    return;
  }
  chromeTop.value = false;
  chromeBottom.value = false;
  showFullscreenSidebar.value = false;
  fullscreenSidebarWidth.value = null;
  resizingSidebar.value = false;
});

watch([maskModalOpen, globalBusy], ([mask, busy]) => {
  if (mask || busy) collapseAllChromeForBlockingOverlay();
});
export const thumbnailsOpen = ref(false);
export const altDown = ref(false);
/** Circular page magnifier (follows pointer; includes OCR overlay). */
export const magnifierOn = ref(false);

export function toggleMagnifier() {
  if (!comic.value) {
    magnifierOn.value = false;
    return;
  }
  magnifierOn.value = !magnifierOn.value;
}

export function setMagnifier(on: boolean) {
  magnifierOn.value = Boolean(on && comic.value);
}
export const dropping = ref(false);
export const clock = ref("");
export const about = ref<AboutInfo | null>(null);

export const pageCount = computed(() => comic.value?.pages.length ?? 0);
export const shownPages = computed(() =>
  pageCount.value <= 0
    ? []
    : spreadIndexes(pageIndex.value, pageCount.value, settings.readingMode, settings.isolateCover),
);
/** Pages that should be OCR'd for the current view (dual spread / scroll viewport / single). */
export const translationTargetPages = computed(() => {
  if (pageCount.value <= 0) return [] as number[];
  if (settings.readingMode === "scroll") {
    const pages = scrollViewportPages.value;
    if (pages.length > 0) return [...pages].sort((a, b) => a - b);
    return [Math.max(0, Math.min(pageCount.value - 1, pageIndex.value))];
  }
  return spreadIndexes(
    pageIndex.value,
    pageCount.value,
    settings.readingMode,
    settings.isolateCover,
  );
});

/** Seek-bar segments for translation progress (purple done / pink running). */
export const translationSeekGradient = computed(() => {
  if (!ocrEnabled.value) return "none";
  const n = pageCount.value;
  if (n <= 0) return "none";
  // Touch reactive map so overlay updates as pages finish.
  void Object.keys(ocrRegionsByPage).length;
  const running = ocrRunningIndex.value;
  type Seg = { start: number; end: number; color: string };
  const segs: Seg[] = [];
  let i = 0;
  while (i < n) {
    const color =
      i === running
        ? "var(--translate-running)"
        : pageHasOcrResult(i)
          ? "var(--translate-done)"
          : "";
    if (!color) {
      i += 1;
      continue;
    }
    let end = i + 1;
    while (
      end < n &&
      (end === running
        ? "var(--translate-running)"
        : pageHasOcrResult(end)
          ? "var(--translate-done)"
          : "") === color
    ) {
      end += 1;
    }
    segs.push({ start: i, end, color });
    i = end;
  }
  if (!segs.length) return "none";
  const pct = (index: number) => {
    if (n <= 1) return index <= 0 ? 0 : 100;
    return Math.min(100, (index / (n - 1)) * 100);
  };
  const parts: string[] = [];
  let cursor = 0;
  for (const seg of segs) {
    if (seg.start > cursor) {
      parts.push(`transparent ${pct(cursor)}%`, `transparent ${pct(seg.start)}%`);
    }
    parts.push(`${seg.color} ${pct(seg.start)}%`, `${seg.color} ${pct(seg.end)}%`);
    cursor = seg.end;
  }
  if (cursor < n) {
    parts.push(`transparent ${pct(cursor)}%`, "transparent 100%");
  }
  return `linear-gradient(to right, ${parts.join(", ")})`;
});

/** Seek-bar segments for extract / PDF rasterize (warning done / lighter running). */
export const extractSeekGradient = computed(() => {
  if (!extracting.value && extractDone.value >= extractTotal.value && extractTotal.value > 0) {
    return "none";
  }
  if (!extracting.value && extractTotal.value <= 0) return "none";
  const n = pageCount.value;
  if (n <= 0) return "none";
  const ready = extractReadyFlags.value;
  const running = extractRunningIndex.value;
  type Seg = { start: number; end: number; color: string };
  const segs: Seg[] = [];
  let i = 0;
  while (i < n) {
    const color =
      i === running
        ? "var(--extract-running)"
        : ready[i]
          ? "var(--extract-done)"
          : "";
    if (!color) {
      i += 1;
      continue;
    }
    let end = i + 1;
    while (
      end < n &&
      (end === running
        ? "var(--extract-running)"
        : ready[end]
          ? "var(--extract-done)"
          : "") === color
    ) {
      end += 1;
    }
    segs.push({ start: i, end, color });
    i = end;
  }
  if (!segs.length) return "none";
  const pct = (index: number) => {
    if (n <= 1) return index <= 0 ? 0 : 100;
    return Math.min(100, (index / (n - 1)) * 100);
  };
  const parts: string[] = [];
  let cursor = 0;
  for (const seg of segs) {
    if (seg.start > cursor) {
      parts.push(`transparent ${pct(cursor)}%`, `transparent ${pct(seg.start)}%`);
    }
    parts.push(`${seg.color} ${pct(seg.start)}%`, `${seg.color} ${pct(seg.end)}%`);
    cursor = seg.end;
  }
  if (cursor < n) {
    parts.push(`transparent ${pct(cursor)}%`, "transparent 100%");
  }
  return `linear-gradient(to right, ${parts.join(", ")})`;
});

export const showExtractBar = computed(
  () => extracting.value || (extractTotal.value > 0 && extractDone.value < extractTotal.value),
);

function resetExtractProgress(total = 0) {
  extractDone.value = 0;
  extractTotal.value = total;
  extractRunningIndex.value = null;
  extractReadyFlags.value = total > 0 ? Array.from({ length: total }, () => false) : [];
}

function scheduleExtractPriority() {
  if (!extracting.value || !comic.value || !inTauri()) return;
  window.clearTimeout(extractPriorityTimer);
  extractPriorityTimer = window.setTimeout(() => {
    void pushExtractPriority();
  }, 40);
}

async function pushExtractPriority() {
  const book = comic.value;
  if (!book || !extracting.value || !inTauri()) return;
  const focus = translationTargetPages.value;
  const count = book.pages.length;
  const pages: number[] = [];
  const seen = new Set<number>();
  const push = (index: number) => {
    if (index < 0 || index >= count || seen.has(index)) return;
    seen.add(index);
    pages.push(index);
  };
  // Visible spread first, then radiate outward (forward before backward):
  // e.g. focus 50 → 50, 51, 49, 52, 48, …
  for (const index of focus) push(index);
  if (!pages.length) push(pageIndex.value);
  const lo = pages.reduce((a, b) => Math.min(a, b), pages[0] ?? pageIndex.value);
  const hi = pages.reduce((a, b) => Math.max(a, b), pages[0] ?? pageIndex.value);
  for (let distance = 1; seen.size < count; distance++) {
    const before = pages.length;
    push(hi + distance);
    push(lo - distance);
    if (pages.length === before) break;
  }
  try {
    await invoke("set_extract_priority", { sessionId: book.sessionId, pages });
  } catch {
    // Extract may have finished between schedule and invoke.
  }
}

export const pageCurrentLabel = computed(() => {
  const pages = shownPages.value.map((page) => page + 1);
  if (pages.length > 1) return `${pages[0]},${pages[pages.length - 1]}`;
  if (pages.length === 1) return `${pages[0]}`;
  return "0";
});
export const pageLabel = computed(() => `${pageCurrentLabel.value} / ${pageCount.value}`);
export const progressIndex = computed(() => {
  const pages = shownPages.value;
  return pages[pages.length - 1] ?? pageIndex.value;
});
export const measureProgress = computed(() => {
  if (measureTotal.value <= 0) return "0%";
  return `${(measureDone.value / measureTotal.value) * 100}%`;
});
export const showMeasureBar = computed(() => {
  if (settings.readingMode !== "scroll") return false;
  if (!comic.value || measureTotal.value <= 0) return false;
  if (measuring.value) return true;
  return measureDone.value > 0 && measureDone.value < measureTotal.value;
});

function pagesNeedMeasure(pages: PageInfo[]) {
  return pages.some((page) => !(page.width > 0 && page.height > 0));
}

function countedMeasured(pages: PageInfo[]) {
  return pages.reduce((sum, page) => sum + (page.width > 0 && page.height > 0 ? 1 : 0), 0);
}

export async function ensureMeasure() {
  const book = comic.value;
  if (!book || !inTauri() || extracting.value) return;
  if (settings.readingMode !== "scroll") return;
  measureTotal.value = book.pages.length;
  const known = countedMeasured(book.pages);
  measureDone.value = known;
  if (!pagesNeedMeasure(book.pages)) {
    measuring.value = false;
    measureDone.value = book.pages.length;
    return;
  }
  if (measuring.value) return;
  measuring.value = true;
  try {
    await invoke("start_measure", { sessionId: book.sessionId });
  } catch {
    measuring.value = false;
  }
}
export const pageBookmarked = computed(() =>
  bookmarks.value.some((bookmark) => bookmark.page === pageIndex.value + 1),
);
export const chapters = computed(() =>
  comic.value
    ? buildChapters(comic.value.pages, settings.chapterRules)
    : { chapters: [], invalidRule: false },
);

export function commitChapterRules(
  rules: ChapterFilenameRule[],
): { ok: true } | { ok: false; errorKey: "invalidRule" | "chapterRuleNeedEnabled" } {
  const compiled = compileEnabledChapterRules(rules);
  if (!compiled.ok) {
    if (compiled.error === "invalid") return { ok: false, errorKey: "invalidRule" };
    return { ok: false, errorKey: "chapterRuleNeedEnabled" };
  }
  settings.chapterRules = cloneChapterRules(rules);
  return { ok: true };
}
export const titleText = computed(() => {
  if (!comic.value || pageCount.value === 0) return i18n.global.t("appName");
  const indexes = shownPages.value.length > 0 ? shownPages.value : [pageIndex.value];
  const pages = indexes
    .map((index) => comic.value?.pages[index])
    .filter((page): page is PageInfo => !!page);
  if (pages.length === 0) return i18n.global.t("appName");
  const container = comic.value.path.split(/[/\\]/).filter(Boolean).pop() || comic.value.title;
  const numbers = indexes.map((index) => index + 1);
  if (pages.length > 1) {
    const names =
      modeFit().direction === "rtl"
        ? [pages[1]?.name, pages[0]?.name]
        : [pages[0]?.name, pages[1]?.name];
    return `[${numbers[0]},${numbers[1]}/${pageCount.value}] ${names[0]} | ${names[1]} <${container}>`;
  }
  return `[${numbers[0]}/${pageCount.value}] ${pages[0]?.name} <${container}>`;
});

let scrollByDelta: ((dy: number, animate?: boolean) => boolean) | null = null;
let playScrollHost: HTMLElement | null = null;
let pageStage: HTMLElement | null = null;
let store: Store | null = null;

export function bindScroll(
  fn: ((dy: number, animate?: boolean) => boolean) | null,
  host: HTMLElement | null = null,
) {
  scrollByDelta = fn;
  playScrollHost = fn ? host : null;
}

export function bindPageStage(el: HTMLElement | null) {
  pageStage = el;
}
let ready = false;
let started = false;
let lastAction = 0;

export function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function asError(error: unknown): string {
  if (typeof error === "string") return error;
  if (error && typeof error === "object" && "message" in error) {
    return String((error as { message: unknown }).message);
  }
  return String(error);
}

function isPathMissingError(message: string) {
  return message === "找不到该路径" || message === "Path not found";
}

function once(run: () => void) {
  const now = Date.now();
  if (now - lastAction < 250) return;
  lastAction = now;
  run();
}

let pendingScrollBehavior: ScrollBehavior = "auto";

export function setIndex(index: number, raw = false, scroll: ScrollBehavior = "smooth") {
  pendingScrollBehavior = scroll;
  const count = pageCount.value;
  if (count <= 0) {
    pageIndex.value = 0;
    return;
  }
  let next = Math.max(0, Math.min(count - 1, Math.trunc(index)));
  if (!raw && settings.readingMode !== "scroll") {
    next = spreadStart(next, count, settings.readingMode, settings.isolateCover);
  }
  pageIndex.value = next;
}

export function consumeScrollBehavior(): ScrollBehavior {
  const scroll = pendingScrollBehavior;
  pendingScrollBehavior = "auto";
  return scroll;
}

export function setZoom(value: number) {
  const next = Number(value);
  settings.zoom = Number.isFinite(next)
    ? Math.max(settings.minZoom, Math.min(settings.maxZoom, next))
    : 1;
}

export function turn(forward: boolean) {
  if (!comic.value) return;
  if (settings.readingMode === "scroll") {
    setIndex(pageIndex.value + (forward ? 1 : -1), true);
    return;
  }
  const spread = spreadIndexes(
    pageIndex.value,
    pageCount.value,
    settings.readingMode,
    settings.isolateCover,
  );
  const delta = (forward ? 1 : -1) * Math.max(1, spread.length);
  setIndex(pageIndex.value + delta);
}

export const playing = ref(false);
let playTimer = 0;

function playViewportPx() {
  if (settings.readingMode === "scroll") {
    return Math.max(1, playScrollHost?.clientHeight ?? 1);
  }
  if (!pageStage) return Math.max(1, window.innerHeight);
  const step = planStageStep(pageStage, true, playRtl());
  if (!step) return Math.max(1, pageStage.clientHeight);
  return Math.max(1, step.axis === "top" ? pageStage.clientHeight : pageStage.clientWidth);
}

export function playScrollStepPx() {
  if (settings.playScrollMode !== "interval") return Math.max(1, settings.playSmoothStep);
  if (settings.playIntervalStepMode === "screen") return playViewportPx();
  return Math.max(1, settings.playIntervalStep);
}

export function playScrollIntervalMs() {
  return settings.playScrollMode === "interval"
    ? Math.max(1, settings.playIntervalMs)
    : Math.max(1, settings.playSmoothInterval);
}

function playRtl() {
  return modeFit().direction === "rtl";
}

function playViewOverflows() {
  if (settings.readingMode === "scroll") {
    const el = playScrollHost;
    return !!el && el.scrollHeight - el.clientHeight > 1;
  }
  return !!pageStage && stageOverflows(pageStage);
}

function playRemaining() {
  if (settings.readingMode === "scroll") {
    const el = playScrollHost;
    if (!el) return 0;
    return Math.max(0, el.scrollHeight - el.clientHeight - el.scrollTop);
  }
  if (!pageStage) return 0;
  return remainingStageScroll(pageStage, true, playRtl());
}

function playNudge() {
  const px = playScrollStepPx();
  const animate = settings.playScrollMode === "interval" && settings.smoothScroll;
  if (settings.readingMode === "scroll") return scrollByDelta?.(px, animate) ?? false;
  if (!pageStage) return false;
  return nudgeStageScroll(pageStage, px, true, playRtl(), animate);
}

let playPaused = false;

function nextPlayWait() {
  return settings.readingMode === "scroll" || playViewOverflows()
    ? playScrollIntervalMs()
    : Math.max(1, settings.playNoScrollPageMs);
}

function stopPlay() {
  playing.value = false;
  playPaused = false;
  window.clearTimeout(playTimer);
  playTimer = 0;
}

export function pausePlayClock() {
  if (!playing.value) return;
  playPaused = true;
  window.clearTimeout(playTimer);
  playTimer = 0;
}

export function resumePlayClock() {
  if (!playing.value || !playPaused) return;
  playPaused = false;
  schedulePlay(nextPlayWait());
}

export function resetPlayClock() {
  if (!playing.value || playPaused) return;
  window.clearTimeout(playTimer);
  schedulePlay(nextPlayWait());
}

function schedulePlay(ms: number) {
  window.clearTimeout(playTimer);
  playTimer = window.setTimeout(() => {
    playTimer = 0;
    runPlayTick();
  }, Math.max(0, ms));
}

function runPlayTick() {
  if (!playing.value || !comic.value || pageCount.value <= 0) {
    stopPlay();
    return;
  }
  if (playNudge()) {
    if (settings.readingMode === "scroll") {
      if (playRemaining() < 1) {
        stopPlay();
        return;
      }
      schedulePlay(playScrollIntervalMs());
      return;
    }
    schedulePlay(playRemaining() < 1 ? Math.max(0, settings.playAfterScrollPageMs) : playScrollIntervalMs());
    return;
  }
  if (settings.readingMode === "scroll") {
    stopPlay();
    return;
  }
  const before = pageIndex.value;
  turn(true);
  if (pageIndex.value === before) {
    stopPlay();
    return;
  }
  schedulePlay(playViewOverflows() ? playScrollIntervalMs() : Math.max(1, settings.playNoScrollPageMs));
}

function armPlay() {
  window.clearTimeout(playTimer);
  playTimer = 0;
  playPaused = false;
  playing.value = true;
  schedulePlay(nextPlayWait());
}

export function togglePlay() {
  if (playing.value) stopPlay();
  else armPlay();
}

watch(
  () => settings.readingMode,
  (mode) => {
    if (mode !== "scroll") {
      window.clearTimeout(scrollResampleTimer);
      scrollResampleReady.value = true;
    }
    if (playing.value) armPlay();
  },
);

watch(comic, (book) => {
  if (!book) {
    stopPlay();
    thumbnailsOpen.value = false;
  }
});

export function toggleSidePanel() {
  if (!comic.value) return;
  if (chromeOverlay.value) {
    showFullscreenSidebar.value = !showFullscreenSidebar.value;
    if (showFullscreenSidebar.value) {
      sideOpen.value = true;
      if (fullscreenSidebarWidth.value == null) {
        fullscreenSidebarWidth.value = clampSidebarWidth(settings.sidebarWidth);
      }
    }
    return;
  }
  sideOpen.value = !sideOpen.value;
}

export function showBookmarks() {
  sideTab.value = "bookmarks";
  if (chromeOverlay.value) {
    sideOpen.value = true;
    revealFullscreenSidebar();
    if (fullscreenSidebarWidth.value == null) {
      fullscreenSidebarWidth.value = clampSidebarWidth(settings.sidebarWidth);
    }
    return;
  }
  sideOpen.value = true;
}

export function showChapters() {
  sideTab.value = "chapters";
  if (chromeOverlay.value) {
    sideOpen.value = true;
    revealFullscreenSidebar();
    if (fullscreenSidebarWidth.value == null) {
      fullscreenSidebarWidth.value = clampSidebarWidth(settings.sidebarWidth);
    }
    return;
  }
  sideOpen.value = true;
}

export function openAddBookmark() {
  if (!comic.value) {
    notifyError(i18n.global.t("noComic"));
    return;
  }
  bookmarkDialog.editIndex = null;
  bookmarkDialog.page = pageIndex.value + 1;
  bookmarkDialog.comment = "";
  bookmarkDialog.open = true;
}

export async function removePageBookmarks() {
  const page = pageIndex.value + 1;
  if (!bookmarks.value.some((bookmark) => bookmark.page === page)) return;
  bookmarks.value = bookmarks.value.filter((bookmark) => bookmark.page !== page);
  await persistBookmarks();
}

export function togglePageBookmark() {
  if (pageBookmarked.value) void removePageBookmarks();
  else openAddBookmark();
}

export function openEditBookmark(index: number) {
  const bookmark = bookmarks.value[index];
  if (!bookmark) return;
  bookmarkDialog.editIndex = index;
  bookmarkDialog.page = bookmark.page;
  bookmarkDialog.comment = bookmark.comment;
  bookmarkDialog.open = true;
}

export async function commitBookmark() {
  if (!comic.value || pageCount.value <= 0) return;
  const page = Math.min(pageCount.value, Math.max(1, Math.trunc(bookmarkDialog.page) || 1));
  bookmarkDialog.page = page;
  const next = { page, comment: bookmarkDialog.comment };
  if (bookmarkDialog.editIndex === null) bookmarks.value.push(next);
  else bookmarks.value.splice(bookmarkDialog.editIndex, 1, next);
  bookmarks.value = parseBookmarks(serializeBookmarks(bookmarks.value));
  bookmarkDialog.open = false;
  await persistBookmarks();
}

export async function deleteBookmark(index: number) {
  bookmarks.value.splice(index, 1);
  await persistBookmarks();
}

export async function clearBookmarks() {
  bookmarks.value = [];
  await persistBookmarks();
}

/** Native Ok/Cancel confirm, then clear. Falls back to window.confirm outside Tauri. */
export async function confirmClearBookmarks() {
  if (bookmarks.value.length === 0) return;
  const message = i18n.global.t("clearBookmarksConfirm");
  const title = i18n.global.t("prompt");
  const ok = inTauri()
    ? await confirm(message, {
        title,
        kind: "warning",
        okLabel: i18n.global.t("ok"),
        cancelLabel: i18n.global.t("cancel"),
      })
    : window.confirm(message);
  if (ok) await clearBookmarks();
}

export function openChapterRules() {
  chapterRuleError.value = "";
  chapterRuleOpen.value = true;
}

export function applyChapterRulesFromPanel(rules: ChapterFilenameRule[]) {
  const result = commitChapterRules(rules);
  if (!result.ok) {
    chapterRuleError.value = i18n.global.t(result.errorKey);
    return;
  }
  chapterRuleError.value = "";
  chapterRuleOpen.value = false;
}

export function closeDialogs() {
  // Mask modals (AppModal) close in open-order stack — topmost first.
  if (closeTopMaskModal()) return true;
  if (colorAdjustOpen.value) {
    colorAdjustOpen.value = false;
    return true;
  }
  if (thumbnailsOpen.value) {
    thumbnailsOpen.value = false;
    return true;
  }
  return false;
}

export function toggleThumbnails() {
  if (!comic.value || pageCount.value <= 0) return;
  thumbnailsOpen.value = !thumbnailsOpen.value;
  if (thumbnailsOpen.value) stopPlay();
}

export async function openArchiveDialog() {
  once(() => {
    void (async () => {
      if (!inTauri()) {
        notifyError(i18n.global.t("desktopOnly"));
        return;
      }
      const selected = await open({
        multiple: false,
        filters: [{ name: "Comic", extensions: [...ARCHIVE_EXTS] }],
      });
      if (typeof selected === "string") await openComic(selected);
    })();
  });
}

export async function openFolderDialog() {
  once(() => {
    void (async () => {
      if (!inTauri()) {
        notifyError(i18n.global.t("desktopOnly"));
        return;
      }
      const selected = await open({ directory: true, multiple: false });
      if (typeof selected === "string") await openComic(selected);
    })();
  });
}

export async function openComic(path: string) {
  if (!inTauri()) {
    notifyError(i18n.global.t("desktopOnly"));
    if (!comic.value) showEmpty.value = true;
    return;
  }
  if (opening.value) return;
  showEmpty.value = false;
  opening.value = true;
  clearAppToasts("error");
  let password: string | undefined;
  try {
    setDemoUrls(null);
    clearPageRotations();
    rotateGuide.value = false;
    while (true) {
      try {
        const info = await invoke<Comic>("open_path", {
          path,
          readSubfolder: settings.readSubfolder,
          pdfRasterize: settings.pdfRasterize,
          locale: i18n.global.locale.value,
          password: password ?? null,
        });
        comic.value = info;
        extracting.value = info.extracting;
        resetExtractProgress(info.extracting ? info.pages.length : 0);
        if (info.extracting) {
          extractReadyFlags.value = Array.from({ length: info.pages.length }, () => false);
          scheduleExtractPriority();
        }
        measuring.value = false;
        measureDone.value = countedMeasured(info.pages);
        measureTotal.value = info.pages.length;
        let index = 0;
        let offset = 0;
        if (info.initialIndex != null) index = info.initialIndex;
        else {
          const previous = history.value.find((item) => item.path === info.path);
          if (previous) {
            index = previous.index;
            offset = clampOffset(previous.offset);
          }
        }
        scrollOffset.value = settings.readingMode === "scroll" ? offset : 0;
        setIndex(index, settings.readingMode === "scroll");
        await loadBookmarkFile();
        await recordRecent();
        await setResumeSession(true);
        void ensureMeasure();
        return;
      } catch (error) {
        const message = asError(error);
        if (isPdfPasswordError(message)) {
          opening.value = false;
          const next = await askPdfPassword(message === PDF_PASSWORD_INCORRECT);
          if (next == null) return;
          password = next;
          opening.value = true;
          continue;
        }
        notifyError(message);
        if (isPathMissingError(message)) await removeHistory(path);
        return;
      }
    }
  } finally {
    opening.value = false;
    if (!comic.value) showEmpty.value = true;
  }
}

export async function revealCurrent() {
  if (!comic.value || !inTauri()) return;
  await revealItemInDir(comic.value.path);
}

export async function closeComic() {
  magnifierOn.value = false;
  if (!comic.value) return;
  window.clearTimeout(rememberTimer);
  stopPlay();
  thumbnailsOpen.value = false;
  showFullscreenSidebar.value = false;
  try {
    await upsertHistory();
    await setResumeSession(false);
  } catch (error) {
    console.error(error);
  }
  setDemoUrls(null);
  comic.value = null;
  pageIndex.value = 0;
  scrollOffset.value = 0;
  extracting.value = false;
  resetExtractProgress(0);
  window.clearTimeout(rememberTimer);
  window.clearTimeout(extractPriorityTimer);
  clearPageRotations();
  clearOcrRegions();
  ocrEnabled.value = false;
  rotateGuide.value = false;
  bookmarks.value = [];
  extracting.value = false;
  measuring.value = false;
  clearAppToasts("error");
  showEmpty.value = true;
  void syncTitle();
}

export async function toggleDevtools() {
  if (!inTauri()) return;
  try {
    await invoke("plugin:webview|internal_toggle_devtools");
  } catch (error) {
    console.error(error);
  }
}

export async function openLogs() {
  if (!inTauri()) return;
  try {
    const { openLogWindow } = await import("./lib/appLog");
    const owner = getCurrentWindow().label || "main";
    await openLogWindow(
      owner,
      i18n.global.t("logs"),
      settings.theme === "light" ? "light" : "dark",
    );
  } catch (error) {
    console.error(error);
  }
}

export async function saveImage(index: number) {
  if (!comic.value) return;
  const page = comic.value.pages[index];
  if (!page) return;
  if (!inTauri()) {
    const link = document.createElement("a");
    link.href = imageSrc(index);
    link.download = page.name;
    link.click();
    notifySuccess(i18n.global.t("saved"), 1600);
    return;
  }
  const ext = page.name.split(".").pop() || "jpg";
  const dest = await save({
    defaultPath: page.name,
    filters: [{ name: "Image", extensions: [ext] }],
  });
  if (typeof dest !== "string") return;
  await invoke("save_page", {
    sessionId: comic.value.sessionId,
    index,
    dest,
  });
  notifySuccess(i18n.global.t("saved"), 1600);
}

export function openImageInfo() {
  if (!comic.value) return;
  imageInfoOpen.value = true;
}

export interface PageFileInfo {
  name: string;
  path?: string | null;
  width?: number | null;
  height?: number | null;
  bitsPerPixel?: number | null;
  format?: string | null;
  color?: string | null;
  bytes?: number | null;
  modifiedMs?: number | null;
  modifiedText?: string | null;
}

export function pageFileInfoSnapshot(index = pageIndex.value): PageFileInfo | null {
  const book = comic.value;
  const page = book?.pages[index];
  if (!book || !page) return null;
  return {
    name: page.name,
    path: joinPagePath(book.path, page.relPath || page.name),
    width: page.width || null,
    height: page.height || null,
  };
}

export async function loadPageFileInfo(index = pageIndex.value): Promise<PageFileInfo | null> {
  const book = comic.value;
  const fallback = pageFileInfoSnapshot(index);
  if (!book || !fallback) return null;
  if (!inTauri()) return fallback;
  try {
    const remote = await invoke<PageFileInfo>("page_file_info", {
      sessionId: book.sessionId,
      index,
    });
    return {
      ...fallback,
      ...remote,
      name: remote.name || fallback.name,
      path: remote.path || fallback.path,
      width: remote.width || fallback.width,
      height: remote.height || fallback.height,
    };
  } catch (error) {
    console.error(error);
    return fallback;
  }
}

function joinPagePath(root: string, rel: string) {
  if (!root) return rel;
  const win = root.includes("\\") || /^[A-Za-z]:/.test(root);
  const sep = win ? "\\" : "/";
  const inner = rel.replace(/[\\/]+/g, sep);
  if (root.endsWith("\\") || root.endsWith("/")) return `${root}${inner}`;
  return `${root}${sep}${inner}`;
}

async function fetchPageBlob(index: number): Promise<Blob> {
  const src = imageSrc(index);
  const response = await fetch(src);
  if (!response.ok) throw new Error(`fetch ${response.status}`);
  return response.blob();
}

async function writeClipboardImage(blob: Blob) {
  if (!navigator.clipboard?.write) throw new Error("clipboard unavailable");
  const type = blob.type && blob.type.startsWith("image/") ? blob.type : "image/png";
  try {
    await navigator.clipboard.write([new ClipboardItem({ [type]: blob })]);
  } catch {
    const png = type === "image/png" ? blob : await blobToPng(blob);
    await navigator.clipboard.write([new ClipboardItem({ "image/png": png })]);
  }
}

async function blobToPng(blob: Blob): Promise<Blob> {
  const bitmap = await createImageBitmap(blob);
  try {
    const canvas = document.createElement("canvas");
    canvas.width = bitmap.width;
    canvas.height = bitmap.height;
    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("canvas unsupported");
    ctx.drawImage(bitmap, 0, 0);
    return await new Promise<Blob>((resolve, reject) => {
      canvas.toBlob((value) => (value ? resolve(value) : reject(new Error("png encode failed"))), "image/png");
    });
  } finally {
    bitmap.close();
  }
}

export async function copyImage(index = pageIndex.value) {
  if (!comic.value) return;
  try {
    await writeClipboardImage(await fetchPageBlob(index));
    notifySuccess(i18n.global.t("copied"), 1600);
  } catch (error) {
    console.error(error);
    notifyError(i18n.global.t("copyFailed"));
  }
}

export async function toggleFullscreen() {
  if (!inTauri()) {
    fullscreen.value = !fullscreen.value;
    return;
  }
  const win = getCurrentWindow();
  const next = !(await win.isFullscreen());
  await win.setFullscreen(next);
  fullscreen.value = next;
}

export async function showAbout() {
  aboutOpen.value = true;
  if (!inTauri()) {
    about.value = { version: "0.1.0", os: "browser", arch: "" };
    return;
  }
  about.value = await invoke<AboutInfo>("system_info");
}

export async function clearHistory() {
  history.value = [];
  await persistHistory();
}

export async function removeHistory(path: string) {
  const next = history.value.filter((item) => item.path !== path);
  if (next.length === history.value.length) return;
  history.value = next;
  await persistHistory();
}

export async function setLocale(next: Locale) {
  i18n.global.locale.value = next;
  await persistSettings();
  await syncTitle();
  if (inTauri()) {
    void import("./lib/appLog").then(({ broadcastUiSync }) => broadcastUiSync({ locale: next }));
  }
}

export { screenScale };

export async function boot() {
  if (started) return;
  started = true;
  ensureFocusTapGuard();
  bindDisplayScale();
  await restore();
  if (store) await initWindowState(store);
  ready = true;
  void refreshTranslationPrereqs();
  void syncTitle();
  tickClock();
  window.setInterval(tickClock, 1000);
  watch(settings, () => {
    if (ready) schedulePersistSettings();
  }, { deep: true });
  // PDF rasterize ↔ extract: cancel in-flight work, drop the other mode's cache, reopen.
  watch(
    () => settings.pdfRasterize,
    (next, prev) => {
      if (prev === undefined || next === prev) return;
      const path = comic.value?.path;
      if (!path || !/\.pdf$/i.test(path)) return;
      void (async () => {
        try {
          await upsertHistory();
        } catch (error) {
          console.error(error);
        }
        await openComic(path);
      })();
    },
  );
  watch(pageIndex, () => {
    scheduleRemember();
    scheduleExtractPriority();
    // Re-prioritize queue after a page turn (neighbors / book batch; do not cancel).
    if (ocrEnabled.value) scheduleTranslateBookResync();
  });
  watch(translationTargetPages, () => {
    scheduleExtractPriority();
  });
  watch(translationTargetPages, (pages, prev) => {
    onTranslationTargetsChanged(pages, prev);
  });
  watch(
    () => settings.translation.bookPreTranslate,
    (next, prev) => {
      if (prev === undefined || next === prev) return;
      scheduleTranslateBookResync();
    },
  );
  // OCR pipeline inputs — disk OCR cache misses; full detect+recognize.
  watch(
    () =>
      [
        settings.translation.ocrEngine,
        settings.translation.sourceLang,
        settings.translation.textDetector,
        settings.translation.enableAuxYolo ? "1" : "0",
        settings.translation.enableSaberYoloRefine ? "1" : "0",
      ].join("\0"),
    (next, prev) => {
      if (prev === undefined || next === prev) return;
      if (ocrBusy.value || ocrEnsureRunning) {
        cancelInFlightOcr();
      }
      clearOcrRegions();
      if (ocrEnabled.value) void ensureShownPagesTranslated();
    },
  );
  // Translate-only inputs — backend reuses OCR-only disk cache, then re-translates.
  watch(
    () =>
      [
        settings.translation.targetLang,
        settings.translation.provider,
        settings.translation.aiBaseUrl,
        settings.translation.aiModel,
      ].join("\0"),
    (next, prev) => {
      if (prev === undefined || next === prev) return;
      if (ocrBusy.value || ocrEnsureRunning) {
        cancelInFlightOcr();
      }
      clearOcrRegions();
      if (ocrEnabled.value) void ensureShownPagesTranslated();
    },
  );
  watch(titleText, () => {
    void syncTitle();
  });
  watch(
    () => [settings.readingMode, settings.isolateCover] as const,
    ([mode]) => {
      if (mode !== "scroll") {
        scrollOffset.value = 0;
        scrollViewportPages.value = [];
      }
      setIndex(pageIndex.value);
      void ensureMeasure();
    },
  );
  watch(extracting, (busy) => {
    if (!busy) void ensureMeasure();
  });
  watch(
    () =>
      [
        comic.value?.sessionId,
        comic.value?.pages.length,
        comic.value?.pages.filter((page) => page.width > 0).length,
        settings.renderProfile,
        settings.neuralTargetSize,
        settings.readSubfolder,
        settings.readingMode,
        settings.isolateCover,
        neuralBatchPaused.value,
        measuring.value,
      ] as const,
    () => {
      scheduleNeuralBook();
    },
  );
  watch(pageIndex, () => {
    scheduleNeuralFocus();
  });
  watch(
    () => [comic.value?.path, settings.syncCurrentFile, settings.readSubfolder] as const,
    () => {
      watchPath = "";
      lastStamp = "";
    },
  );
  window.setInterval(() => {
    void pollCurrentStamp();
  }, 1500);
  if (await loadDemo()) return;
  if (!inTauri()) {
    showEmpty.value = true;
    return;
  }
  await listen<string>("open-path", (event) => {
    const path = event.payload?.trim();
    if (path) void openComic(path);
  });
  const launchPath = await invoke<string | null>("take_pending_open");
  // Only the first live reader process auto-resumes; later instances start empty
  // (unless launched with an explicit file path).
  const isPrimary = await invoke<boolean>("is_primary_instance");
  const resume =
    isPrimary && !launchPath && settings.autoLoadHistory && resumeSession
      ? history.value[0]?.path
      : undefined;
  if (!launchPath && !resume) showEmpty.value = true;
  await listen<ExtractProgress>("extract-progress", (event) => {
    const progress = event.payload;
    if (!comic.value || progress.sessionId !== comic.value.sessionId) return;
    extractDone.value = progress.done;
    extractTotal.value = progress.total;
    if (progress.total > 0 && extractReadyFlags.value.length !== progress.total) {
      extractReadyFlags.value = Array.from({ length: progress.total }, (_, i) => !!extractReadyFlags.value[i]);
    }
    if (progress.index != null && progress.index >= 0) {
      const next = extractReadyFlags.value.slice();
      next[progress.index] = true;
      extractReadyFlags.value = next;
    }
    extractRunningIndex.value =
      progress.finished || progress.running == null ? null : progress.running;
    if (progress.finished) {
      extracting.value = false;
      extractRunningIndex.value = null;
      if (progress.total > 0) {
        extractReadyFlags.value = Array.from({ length: progress.total }, () => true);
      }
    }
    if (progress.error && !/cancelled/i.test(progress.error)) {
      notifyError(progress.error);
    }
  });
  function applyPageMetric(book: Comic, metric: PageMetric) {
    const page = book.pages[metric.index];
    if (!page) return false;
    let changed = false;
    if (page.width !== metric.width || page.height !== metric.height) {
      page.width = metric.width;
      page.height = metric.height;
      changed = true;
    }
    if (metric.name && page.name !== metric.name) {
      page.name = metric.name;
      changed = true;
    }
    if (metric.relPath && page.relPath !== metric.relPath) {
      page.relPath = metric.relPath;
      changed = true;
    }
    return changed;
  }
  await listen<PageMetric>("page-metric", (event) => {
    const metric = event.payload;
    const book = comic.value;
    if (!book || metric.sessionId !== book.sessionId) return;
    if (applyPageMetric(book, metric) && measureTotal.value > 0) {
      measureDone.value = countedMeasured(book.pages);
    }
  });
  await listen<PageMetricsBatch>("page-metrics", (event) => {
    const batch = event.payload;
    const book = comic.value;
    if (!book || batch.sessionId !== book.sessionId) return;
    for (const metric of batch.pages) applyPageMetric(book, metric);
    if (measureTotal.value > 0) {
      measureDone.value = countedMeasured(book.pages);
    }
  });
  await listen<MeasureProgress>("measure-progress", (event) => {
    const progress = event.payload;
    if (!comic.value || progress.sessionId !== comic.value.sessionId) return;
    measureTotal.value = progress.total;
    measureDone.value = Math.max(measureDone.value, progress.done);
    if (progress.finished) {
      measuring.value = false;
      measureDone.value = progress.total;
    } else {
      measuring.value = true;
    }
  });
  const win = getCurrentWindow();
  /** True only while an OS file drag is active (ignore in-app text selection drags). */
  let fileDragActive = false;
  await win.onDragDropEvent((event) => {
    const payload = event.payload;
    if (payload.type === "enter") {
      fileDragActive = payload.paths.length > 0;
      dropping.value = fileDragActive;
      return;
    }
    if (payload.type === "over") {
      if (fileDragActive) dropping.value = true;
      return;
    }
    if (payload.type === "leave") {
      fileDragActive = false;
      dropping.value = false;
      return;
    }
    if (payload.type === "drop") {
      fileDragActive = false;
      dropping.value = false;
      const dropped = payload.paths[0];
      if (dropped) void openComic(dropped);
    }
  });
  // Persist only — paired log window is closed from Rust on reader Destroyed
  // (beforeunload also fires on page refresh and must not close the log window).
  window.addEventListener("beforeunload", () => {
    void rememberSession();
    void persistSettings();
    void persistWindowState();
  });
  if (launchPath) await openComic(launchPath);
  else if (resume) await openComic(resume);
}

let rememberTimer = 0;
let watchPath = "";
let lastStamp = "";
let reloadingCurrent = false;

async function pollCurrentStamp() {
  if (!settings.syncCurrentFile || !inTauri() || !comic.value) return;
  if (opening.value || extracting.value || reloadingCurrent) return;
  const path = comic.value.path;
  try {
    const stamp = await invoke<string>("path_stamp", {
      path,
      readSubfolder: settings.readSubfolder,
    });
    if (watchPath !== path) {
      watchPath = path;
      lastStamp = stamp;
      return;
    }
    if (stamp === lastStamp) return;
    lastStamp = stamp;
    reloadingCurrent = true;
    const index = pageIndex.value;
    const offset = scrollOffset.value;
    const title = comic.value.title;
    history.value = [
      { path, index, offset, title },
      ...history.value.filter((entry) => entry.path !== path),
    ];
    await openComic(path);
  } catch {
    // Keep the current session if the path cannot be read.
  } finally {
    reloadingCurrent = false;
  }
}

function clampOffset(value: number | undefined): number {
  if (typeof value !== "number" || !Number.isFinite(value)) return 0;
  return Math.min(1, Math.max(0, value));
}

export function scheduleRemember() {
  if (!ready || !comic.value) return;
  window.clearTimeout(rememberTimer);
  rememberTimer = window.setTimeout(() => void rememberSession(), 400);
}

async function syncCacheDirFromBackend() {
  if (!inTauri()) return;
  try {
    // Resolved path is for UI only — do not write it back as a custom pin.
    settings.cacheDir = await getCacheDir();
  } catch {
    settings.cacheDir = "";
  }
}

async function restore() {
  if (inTauri()) {
    const storePath = await getReaderStorePath();
    store = await load(storePath, { autoSave: 200, defaults: {} });
    const savedSettings = await store.get<Partial<Settings>>("settings");
    const savedHistory = await store.get<HistoryItem[]>("history");
    const savedLocale = await store.get<Locale>("locale");
    const savedResume = await store.get<boolean>("resumeSession");
    applySettings(savedSettings ?? {});
    // Never treat a previously saved absolute cacheDir as a custom pin.
    settings.cacheDir = "";
    settings.fileAssociations = normalizeFileAssociations(settings.fileAssociations);
    settings.contextMenu = !!settings.contextMenu;
    setZoom(settings.zoom);
    history.value = savedHistory ?? [];
    resumeSession = savedResume !== false;
    if (isLocale(savedLocale)) i18n.global.locale.value = savedLocale;
    else {
      i18n.global.locale.value = detectSystemLocale();
      await store.set("locale", i18n.global.locale.value);
    }
    settings.translation = await hydrateTranslationSecretsFromVault(settings.translation);
    await syncCacheDirFromBackend();
    alignCanvas();
    return;
  }
  const raw = localStorage.getItem("supermanga");
  if (!raw) {
    i18n.global.locale.value = detectSystemLocale();
    return;
  }
  const saved = JSON.parse(raw) as {
    settings?: Partial<Settings>;
    history?: HistoryItem[];
    locale?: Locale;
    resumeSession?: boolean;
  };
  applySettings(saved.settings ?? {});
  settings.fileAssociations = normalizeFileAssociations(settings.fileAssociations);
  settings.contextMenu = !!settings.contextMenu;
  setZoom(settings.zoom);
  history.value = saved.history ?? [];
  resumeSession = saved.resumeSession !== false;
  if (isLocale(saved.locale)) i18n.global.locale.value = saved.locale;
  else i18n.global.locale.value = detectSystemLocale();
  alignCanvas();
}

let settingsPersistTimer = 0;
let settingsPersistChain: Promise<void> = Promise.resolve();

function schedulePersistSettings() {
  if (!ready) return;
  window.clearTimeout(settingsPersistTimer);
  settingsPersistTimer = window.setTimeout(() => {
    void persistSettings();
  }, 250);
}

async function persistSettings() {
  if (!ready) return;
  window.clearTimeout(settingsPersistTimer);
  settingsPersistChain = settingsPersistChain
    .catch(() => {})
    .then(async () => {
      // Sync into a copy only — never reassign settings.translation here.
      // Replacing that object re-fires the deep settings watch + OCR clear watch,
      // which made the overlay flicker between empty and covered forever.
      const translationForDisk = syncAiApiKeyIntoVendorSlot(settings.translation);
      if (store) {
        let vaultOk = false;
        try {
          await persistTranslationSecretsToVault(translationForDisk);
          vaultOk = true;
        } catch {
          vaultOk = false;
        }
        // Only strip secrets from reader.json after the vault/file backup succeeded.
        // Keep cacheDir empty on disk so portable default is not pinned to an absolute path.
        const diskSettings = {
          ...settings,
          cacheDir: "",
          translation: vaultOk
            ? stripTranslationSecretsForDisk(translationForDisk)
            : translationForDisk,
        };
        await store.set("settings", diskSettings);
        await store.set("locale", i18n.global.locale.value);
        // autoSave is debounced; flush so close/restart cannot drop AI vendor/model.
        await store.save();
        return;
      }
      writeLocal();
    });
  await settingsPersistChain;
}

async function persistHistory() {
  if (store) {
    await store.set("history", history.value);
    return;
  }
  writeLocal();
}

function writeLocal() {
  localStorage.setItem(
    "supermanga",
    JSON.stringify({
      settings: { ...settings, cacheDir: "" },
      history: history.value,
      locale: i18n.global.locale.value,
      resumeSession,
    }),
  );
}

async function setResumeSession(next: boolean) {
  if (resumeSession === next) return;
  resumeSession = next;
  if (!ready) return;
  if (store) {
    await store.set("resumeSession", next);
    return;
  }
  writeLocal();
}

async function upsertHistory() {
  if (!comic.value || pageCount.value === 0) return;
  if (settings.historyCount <= 0) {
    if (history.value.length) {
      history.value = [];
      await persistHistory();
    }
    return;
  }
  const offset = settings.readingMode === "scroll" ? clampOffset(scrollOffset.value) : 0;
  const item: HistoryItem = {
    path: comic.value.path,
    index: pageIndex.value,
    offset,
    title: comic.value.title,
  };
  const head = history.value[0];
  if (
    head &&
    head.path === item.path &&
    head.index === item.index &&
    head.title === item.title &&
    Math.abs(clampOffset(head.offset) - offset) < 1e-4
  ) {
    return;
  }
  history.value = [item, ...history.value.filter((entry) => entry.path !== item.path)].slice(
    0,
    Math.max(0, settings.historyCount),
  );
  await persistHistory();
}

async function recordRecent() {
  await upsertHistory();
}

async function rememberSession() {
  if (!settings.autoLoadHistory) return;
  await upsertHistory();
}

async function loadBookmarkFile() {
  if (!comic.value || !inTauri()) {
    bookmarks.value = [];
    return;
  }
  const text = await invoke<string>("load_bookmarks", { path: comic.value.path });
  bookmarks.value = parseBookmarks(text);
}

async function persistBookmarks() {
  if (!comic.value || !inTauri()) return;
  if (bookmarks.value.length === 0) {
    await invoke("delete_bookmarks", { path: comic.value.path });
    return;
  }
  await invoke("save_bookmarks", {
    path: comic.value.path,
    content: serializeBookmarks(bookmarks.value),
  });
}

async function syncTitle() {
  const title = titleText.value;
  document.title = title;
  if (inTauri()) await getCurrentWindow().setTitle(title);
}

function tickClock() {
  const clockLocale = { "zh-CN": "zh-CN", "zh-TW": "zh-TW", en: "en-GB", ja: "ja-JP", ko: "ko-KR" }[i18n.global.locale.value];
  clock.value = new Date().toLocaleTimeString(clockLocale, {
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  });
}

export async function exitApp() {
  try {
    await rememberSession();
    await persistSettings();
    await persistWindowState();
  } catch (error) {
    console.error(error);
  }
  if (inTauri()) await getCurrentWindow().close();
}

export async function relaunchApp() {
  try {
    await rememberSession();
    await persistSettings();
    await persistWindowState();
  } catch (error) {
    console.error(error);
  }
  if (!inTauri()) {
    window.location.reload();
    return;
  }
  await invoke("relaunch_app");
}

export function imageSrc(index: number): string {
  if (!comic.value) return "";
  return pageUrl(comic.value.protocolOrigin, comic.value.sessionId, index);
}

async function loadDemo(): Promise<boolean> {
  if (!import.meta.env.DEV) return false;
  if (!new URLSearchParams(location.search).has("demo")) return false;
  const colors = ["#243044", "#3a2a22", "#1e3328", "#332433", "#2c2c2c"];
  const urls: string[] = [];
  const pages: PageInfo[] = [];
  for (let index = 0; index < 24; index += 1) {
    const chapter = Math.floor(index / 4) + 1;
    const number = (index % 4) + 1;
    const name = `${chapter}_${String(number).padStart(2, "0")}.jpg`;
    const height = 700 + ((index * 137) % 900);
    urls.push(await paintDemo(index, 900, height, colors[index % colors.length] ?? "#222", name));
    pages.push({ name, relPath: name, width: 900, height });
  }
  setDemoUrls(urls);
  comic.value = {
    sessionId: 1,
    title: "Demo",
    path: "demo",
    pages,
    extracting: false,
    initialIndex: null,
    protocolOrigin: "demo",
  };
  pageIndex.value = 0;
  scrollOffset.value = 0;
  void syncTitle();
  return true;
}

function paintDemo(index: number, width: number, height: number, color: string, name: string) {
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const context = canvas.getContext("2d");
  if (!context) return Promise.resolve("");
  context.fillStyle = color;
  context.fillRect(0, 0, width, height);
  context.strokeStyle = "rgba(255,255,255,.45)";
  context.lineWidth = 8;
  context.strokeRect(24, 24, width - 48, height - 48);
  context.fillStyle = "#f3f3f3";
  context.font = "72px sans-serif";
  context.fillText(String(index + 1), 48, 140);
  context.font = "32px sans-serif";
  context.fillText(name, 48, 200);
  return new Promise<string>((resolve) => {
    canvas.toBlob((blob) => resolve(blob ? URL.createObjectURL(blob) : ""), "image/png");
  });
}

function runShortcutAction(action: ShortcutActionId) {
  switch (action) {
    case "openArchive":
      once(() => {
        void openArchiveDialog();
      });
      return;
    case "openFolder":
      once(() => {
        void openFolderDialog();
      });
      return;
    case "saveImage":
      if (!comic.value) return;
      once(() => {
        void saveImage(pageIndex.value);
      });
      return;
    case "copyImage":
      if (!comic.value) return;
      once(() => {
        void copyImage();
      });
      return;
    case "closeComic":
      if (!comic.value) return;
      once(() => {
        void closeComic();
      });
      return;
    case "closeWindow":
      once(() => {
        void exitApp();
      });
      return;
    case "settings":
      once(() => {
        openSettings();
      });
      return;
    case "logs":
      once(() => {
        void openLogs();
      });
      return;
    case "theme":
      once(() => toggleTheme());
      return;
    case "minimalist":
      if (fullscreen.value) return;
      once(() => toggleMinimalist());
      return;
    case "colorAdjust":
      once(() => {
        colorAdjustOpen.value = !colorAdjustOpen.value;
      });
      return;
    case "ocrTranslate":
      if (!comic.value || (!ocrEnabled.value && !canEnableTranslation())) return;
      once(() => {
        void toggleTranslationMode();
      });
      return;
    case "ocrRetranslate":
      if (!comic.value || !canEnableTranslation()) return;
      once(() => {
        void retranslateCurrentPage();
      });
      return;
    case "fullscreen":
      void toggleFullscreen();
      return;
    case "panel":
      if (!comic.value) return;
      once(() => toggleSidePanel());
      return;
    case "thumbnails":
      if (!comic.value) return;
      once(() => toggleThumbnails());
      return;
    case "magnifier":
      if (!comic.value) return;
      once(() => toggleMagnifier());
      return;
    case "bookmark":
      if (!comic.value) return;
      once(() => togglePageBookmark());
      return;
    case "rotate":
      if (!comic.value) return;
      once(() => rotatePageClockwise());
      return;
    case "rotateCcw":
      if (!comic.value) return;
      once(() => rotatePageCounterClockwise());
      return;
    case "play":
      if (!comic.value) return;
      once(() => togglePlay());
      return;
    case "first":
      if (!comic.value) return;
      setIndex(0, settings.readingMode === "scroll");
      return;
    case "last":
      if (!comic.value) return;
      setIndex(pageCount.value - 1, settings.readingMode === "scroll");
      return;
    case "prev":
      if (!comic.value) return;
      if (settings.readingMode === "scroll") setIndex(pageIndex.value - 1, true);
      else turn(false);
      return;
    case "next":
      if (!comic.value) return;
      if (settings.readingMode === "scroll") setIndex(pageIndex.value + 1, true);
      else turn(true);
      return;
    case "scrollUp":
      if (!comic.value || settings.readingMode !== "scroll") return;
      resetPlayClock();
      scrollByDelta?.(-scrollStepPx());
      return;
    case "scrollDown":
      if (!comic.value || settings.readingMode !== "scroll") return;
      resetPlayClock();
      scrollByDelta?.(scrollStepPx());
      return;
  }
}

export function handleKey(event: KeyboardEvent) {
  // WebView Find-in-page (F3 / Ctrl+F); also blocked at host on Windows.
  if (
    event.key === "F3"
    || ((event.ctrlKey || event.metaKey) && !event.altKey && event.key.toLowerCase() === "f")
  ) {
    event.preventDefault();
  }
  const target = event.target;
  if (target instanceof HTMLElement) {
    const tag = target.tagName;
    if (tag === "INPUT" || tag === "TEXTAREA" || target.isContentEditable) return;
  }
  if (event.key === "Alt") {
    event.preventDefault();
    altDown.value = true;
    return;
  }
  if (event.key === "Escape") {
    if (runtimeRestartOpen.value) {
      event.preventDefault();
      return;
    }
    if (closeDialogs()) {
      event.preventDefault();
      return;
    }
    if (fullscreen.value) {
      event.preventDefault();
      void toggleFullscreen();
      return;
    }
    if (settings.minimalist) {
      event.preventDefault();
      toggleMinimalist();
    }
    return;
  }
  if (
    runtimeRestartOpen.value
    || pdfPasswordPrompt.value
    || settingsOpen.value
    || shortcutsOpen.value
    || bookmarkDialog.open
    || chapterRuleOpen.value
    || aboutOpen.value
    || ocrAssetsOpen.value
    || detectorAssetsOpen.value
    || neuralAssetsOpen.value
  ) {
    return;
  }
  const accel = keyboardEventToAccelerator(event);
  if (!accel) return;
  if (normalizeAccelerator(accel) === normalizeAccelerator(FIXED_DEVTOOLS_ACCEL)) {
    event.preventDefault();
    void toggleDevtools();
    return;
  }
  const action = matchShortcutAction(accel, shortcutBindings.value);
  if (!action) return;
  if (colorAdjustOpen.value && action !== "colorAdjust") return;
  if (
    (action === "scrollUp" || action === "scrollDown")
    && (event.key === "ArrowUp" || event.key === "ArrowDown")
  ) {
    return;
  }
  if (action === "copyImage" && (document.getSelection()?.toString().trim() ?? "")) {
    return;
  }
  event.preventDefault();
  runShortcutAction(action);
}

export function handleKeyUp(event: KeyboardEvent) {
  if (event.key === "Alt") altDown.value = false;
}
