//! Restore main window geometry before the first paint to avoid a default-size flash.

use serde::Deserialize;
use serde_json::Value;
use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, WebviewWindow};

use crate::app_cache;

const MIN_WIDTH: f64 = 720.0;
const MIN_HEIGHT: f64 = 480.0;

#[derive(Debug, Deserialize)]
struct WindowState {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    #[serde(default)]
    maximized: bool,
}

fn load_state(app: &AppHandle) -> Option<WindowState> {
    let path = app_cache::reader_store_path(app).ok()?;
    let bytes = std::fs::read(path).ok()?;
    let root: Value = serde_json::from_slice(&bytes).ok()?;
    let state: WindowState = serde_json::from_value(root.get("window")?.clone()).ok()?;
    if !state.x.is_finite()
        || !state.y.is_finite()
        || !state.width.is_finite()
        || !state.height.is_finite()
    {
        return None;
    }
    Some(state)
}

fn clamp_size(width: f64, height: f64) -> (f64, f64) {
    (width.max(MIN_WIDTH), height.max(MIN_HEIGHT))
}

fn apply(window: &WebviewWindow, state: &WindowState) {
    let (width, height) = clamp_size(state.width, state.height);
    // Values in reader.json are logical (CSS) pixels — match the JS saver.
    let _ = window.set_size(LogicalSize::new(width, height));
    let _ = window.set_position(LogicalPosition::new(state.x, state.y));
    if state.maximized {
        let _ = window.maximize();
    }
}

/// Stop WebView2 from hijacking keys like F3 / Ctrl+F for Find-in-page.
#[cfg(windows)]
fn disable_browser_accelerator_keys(window: &WebviewWindow) {
    let _ = window.with_webview(|webview| {
        use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings3;
        use windows::core::Interface;

        unsafe {
            let Ok(core) = webview.controller().CoreWebView2() else {
                return;
            };
            let Ok(settings) = core.Settings() else {
                return;
            };
            if let Ok(settings3) = settings.cast::<ICoreWebView2Settings3>() {
                let _ = settings3.SetAreBrowserAcceleratorKeysEnabled(false);
            }
        }
    });
}

/// Tauri loads the app ICO at `SM_CXICON` (32px @96DPI) then applies it via `WM_SETICON` as
/// `ICON_SMALL`. The title bar needs `SM_CXSMICON` (16px), so Windows downscales and the glyph
/// looks soft — unlike NW.js, which leaves the multi-size ICO to the shell.
///
/// Reload both sizes straight from the embedded resource with `LoadImageW` (no RGBA round-trip).
/// Call for every native window (main + secondary) after it is created.
#[cfg(windows)]
pub fn apply_native_window_icons(window: &WebviewWindow) {
    use windows::core::PCWSTR;
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, LoadImageW, SendMessageW, ICON_BIG, ICON_SMALL, IMAGE_ICON,
        LR_DEFAULTCOLOR, SM_CXICON, SM_CXSMICON, SM_CYICON, SM_CYSMICON, WM_SETICON,
    };

    // Same id `tauri-build` writes into resource.rc (`32512 ICON "…/icon.ico"`).
    const APP_ICON_RESOURCE_ID: usize = 32512;

    let Ok(hwnd) = window.hwnd() else {
        return;
    };

    unsafe {
        let Ok(module) = GetModuleHandleW(PCWSTR::null()) else {
            return;
        };
        let metric = |index| match GetSystemMetrics(index) {
            n if n > 0 => n,
            _ => 0,
        };
        let load = |cx, cy, fallback| {
            let (w, h) = if cx > 0 && cy > 0 {
                (cx, cy)
            } else {
                (fallback, fallback)
            };
            LoadImageW(
                Some(module.into()),
                PCWSTR(APP_ICON_RESOURCE_ID as *const u16),
                IMAGE_ICON,
                w,
                h,
                LR_DEFAULTCOLOR,
            )
            .ok()
        };

        if let Some(small) = load(metric(SM_CXSMICON), metric(SM_CYSMICON), 16) {
            let _ = SendMessageW(
                hwnd,
                WM_SETICON,
                Some(windows::Win32::Foundation::WPARAM(ICON_SMALL as usize)),
                Some(windows::Win32::Foundation::LPARAM(small.0 as isize)),
            );
        }
        if let Some(big) = load(metric(SM_CXICON), metric(SM_CYICON), 32) {
            let _ = SendMessageW(
                hwnd,
                WM_SETICON,
                Some(windows::Win32::Foundation::WPARAM(ICON_BIG as usize)),
                Some(windows::Win32::Foundation::LPARAM(big.0 as isize)),
            );
        }
    }
}

/// Apply saved geometry (if any) then show the main window.
pub fn restore_and_show(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    #[cfg(windows)]
    disable_browser_accelerator_keys(&window);
    #[cfg(windows)]
    apply_native_window_icons(&window);
    if let Some(state) = load_state(app) {
        apply(&window, &state);
    }
    let _ = window.show();
    let _ = window.set_focus();
}
