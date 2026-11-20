/** Persist main window size / position. Restore happens in Rust before first show. */

import { getCurrentWindow } from "@tauri-apps/api/window";
import type { Store } from "@tauri-apps/plugin-store";
import { syncDisplayScale } from "./displayScale";

const STORE_KEY = "window";
const MIN_WIDTH = 720;
const MIN_HEIGHT = 480;
const SAVE_MS = 300;

export type WindowState = {
  x: number;
  y: number;
  width: number;
  height: number;
  maximized: boolean;
};

let storeRef: Store | null = null;
let saveTimer = 0;
let normal: Pick<WindowState, "x" | "y" | "width" | "height"> | null = null;
let listening = false;

function isState(value: unknown): value is WindowState {
  if (!value || typeof value !== "object") return false;
  const item = value as Record<string, unknown>;
  return (
    typeof item.x === "number" &&
    typeof item.y === "number" &&
    typeof item.width === "number" &&
    typeof item.height === "number" &&
    typeof item.maximized === "boolean" &&
    Number.isFinite(item.x) &&
    Number.isFinite(item.y) &&
    Number.isFinite(item.width) &&
    Number.isFinite(item.height)
  );
}

async function readGeometry(): Promise<WindowState> {
  const win = getCurrentWindow();
  const factor = await win.scaleFactor();
  const pos = await win.outerPosition();
  // setSize restores inner size; outerSize includes the Win32 frame and grows each launch.
  const size = await win.innerSize();
  const maximized = await win.isMaximized();
  return {
    x: Math.round(pos.x / factor),
    y: Math.round(pos.y / factor),
    width: Math.round(size.width / factor),
    height: Math.round(size.height / factor),
    maximized,
  };
}

async function writeStore(state: WindowState) {
  if (!storeRef) return;
  await storeRef.set(STORE_KEY, state);
}

export async function persistWindowState() {
  if (!storeRef) return;
  try {
    if (await getCurrentWindow().isMinimized()) return;
    const current = await readGeometry();
    if (!current.maximized) {
      normal = {
        x: current.x,
        y: current.y,
        width: current.width,
        height: current.height,
      };
    }
    const base = normal ?? current;
    await writeStore({
      x: base.x,
      y: base.y,
      width: Math.max(MIN_WIDTH, base.width),
      height: Math.max(MIN_HEIGHT, base.height),
      maximized: current.maximized,
    });
  } catch (error) {
    console.error("persist window state failed", error);
  }
}

function schedulePersist() {
  window.clearTimeout(saveTimer);
  saveTimer = window.setTimeout(() => {
    void persistWindowState();
  }, SAVE_MS);
}

export async function initWindowState(store: Store) {
  storeRef = store;
  // Geometry is restored on the Rust side before the window is shown.
  try {
    const saved = await store.get<unknown>(STORE_KEY);
    if (isState(saved)) {
      normal = {
        x: saved.x,
        y: saved.y,
        width: Math.max(MIN_WIDTH, saved.width),
        height: Math.max(MIN_HEIGHT, saved.height),
      };
    } else {
      const current = await readGeometry();
      if (!current.maximized) {
        normal = {
          x: current.x,
          y: current.y,
          width: current.width,
          height: current.height,
        };
      }
    }
  } catch (error) {
    console.error("init window state failed", error);
  }

  if (listening) return;
  listening = true;
  const win = getCurrentWindow();
  await win.onResized(() => schedulePersist());
  await win.onMoved(() => schedulePersist());
  await win.onScaleChanged(() => {
    syncDisplayScale();
    schedulePersist();
  });
}
