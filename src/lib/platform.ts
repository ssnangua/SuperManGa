/** Best-effort OS check for desktop WebView / browser preview. */
export function isWindowsDesktop(): boolean {
  if (typeof navigator === "undefined") return false;
  return /windows/i.test(navigator.userAgent);
}

/** Best-effort macOS check (Tauri webview / browser preview). */
export function isMacDesktop(): boolean {
  if (typeof navigator === "undefined") return false;
  return /mac/i.test(navigator.platform) || /Mac OS X/i.test(navigator.userAgent);
}
