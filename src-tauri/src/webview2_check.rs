//! Portable builds have no NSIS/MSI WebView2 bootstrapper. Detect Evergreen
//! Runtime before Tauri creates a window; if missing, show a native prompt.

use windows::core::{w, PCWSTR};

const DOWNLOAD_URL: &str = "https://developer.microsoft.com/microsoft-edge/webview2/";
const BTN_DOWNLOAD: i32 = 1001;
const BTN_CANCEL: i32 = 1002;

struct PromptCopy {
    window_title: PCWSTR,
    /// TaskDialog content; may include `<a href="...">…</a>`.
    content: PCWSTR,
    /// MessageBox fallback without markup.
    content_plain: PCWSTR,
    download: PCWSTR,
    cancel: PCWSTR,
}

/// Returns `true` when WebView2 is available and the app may continue.
pub fn ensure_runtime_or_prompt() -> bool {
    if runtime_available() {
        return true;
    }
    prompt_and_maybe_open_download();
    false
}

fn runtime_available() -> bool {
    use webview2_com::Microsoft::Web::WebView2::Win32::GetAvailableCoreWebView2BrowserVersionString;
    use webview2_com::take_pwstr;
    use windows::core::PWSTR;

    let mut version = PWSTR::null();
    let ok = unsafe {
        GetAvailableCoreWebView2BrowserVersionString(PCWSTR::null(), &mut version).is_ok()
    };
    if !ok {
        return false;
    }
    !take_pwstr(version).is_empty()
}

fn prompt_copy() -> PromptCopy {
    use windows::Win32::Globalization::GetUserDefaultUILanguage;

    // LANGID: primary in low 10 bits, sublang in bits 10–15.
    let langid = unsafe { GetUserDefaultUILanguage() };
    let primary = langid & 0x3ff;
    let sub = langid >> 10;

    match primary {
        // Chinese
        0x04 => {
            // Traditional: Taiwan / HK / Macau (and explicit Traditional sublang).
            if matches!(sub, 0x01 | 0x03 | 0x05) {
                PromptCopy {
                    window_title: w!("SuperManGa"),
                    content: w!("缺少 Microsoft Edge WebView2 執行階段，程式無法啟動。請前往 <a href=\"https://developer.microsoft.com/microsoft-edge/webview2/\">微軟下載頁面</a>，下載安裝後再重新開啟本程式。"),
                    content_plain: w!("缺少 Microsoft Edge WebView2 執行階段，程式無法啟動。請前往微軟下載頁面，下載安裝後再重新開啟本程式。"),
                    download: w!("前往下載"),
                    cancel: w!("取消"),
                }
            } else {
                PromptCopy {
                    window_title: w!("SuperManGa"),
                    content: w!("缺少 Microsoft Edge WebView2 运行时，程序无法启动。请前往 <a href=\"https://developer.microsoft.com/microsoft-edge/webview2/\">微软下载页面</a>，下载安装后再重新打开本程序。"),
                    content_plain: w!("缺少 Microsoft Edge WebView2 运行时，程序无法启动。请前往微软下载页面，下载安装后再重新打开本程序。"),
                    download: w!("前往下载"),
                    cancel: w!("取消"),
                }
            }
        }
        // Japanese
        0x11 => PromptCopy {
            window_title: w!("SuperManGa"),
            content: w!("Microsoft Edge WebView2 ランタイムがないため、アプリを起動できません。<a href=\"https://developer.microsoft.com/microsoft-edge/webview2/\">Microsoft のダウンロードページ</a>からインストールしてから、もう一度開いてください。"),
            content_plain: w!("Microsoft Edge WebView2 ランタイムがないため、アプリを起動できません。Microsoft のダウンロードページからインストールしてから、もう一度開いてください。"),
            download: w!("ダウンロードへ"),
            cancel: w!("キャンセル"),
        },
        // Korean
        0x12 => PromptCopy {
            window_title: w!("SuperManGa"),
            content: w!("Microsoft Edge WebView2 런타임이 없어 프로그램을 시작할 수 없습니다. <a href=\"https://developer.microsoft.com/microsoft-edge/webview2/\">Microsoft 다운로드 페이지</a>에서 설치한 뒤 다시 열어 주세요."),
            content_plain: w!("Microsoft Edge WebView2 런타임이 없어 프로그램을 시작할 수 없습니다. Microsoft 다운로드 페이지에서 설치한 뒤 다시 열어 주세요."),
            download: w!("다운로드로 이동"),
            cancel: w!("취소"),
        },
        // English + everything else (matches frontend locale fallback).
        _ => PromptCopy {
            window_title: w!("SuperManGa"),
            content: w!("Microsoft Edge WebView2 Runtime is missing, so the app cannot start. Open the <a href=\"https://developer.microsoft.com/microsoft-edge/webview2/\">Microsoft download page</a>, install it, then open this app again."),
            content_plain: w!("Microsoft Edge WebView2 Runtime is missing, so the app cannot start. Open the Microsoft download page, install it, then open this app again."),
            download: w!("Download"),
            cancel: w!("Cancel"),
        },
    }
}

fn prompt_and_maybe_open_download() {
    if show_task_dialog() {
        return;
    }
    // Fallback if comctl TaskDialog is unavailable.
    show_message_box_fallback();
}

fn show_task_dialog() -> bool {
    use windows::Win32::UI::Controls::{
        TaskDialogIndirect, TASKDIALOGCONFIG, TASKDIALOGCONFIG_0, TASKDIALOG_BUTTON,
        TDF_ALLOW_DIALOG_CANCELLATION, TDF_ENABLE_HYPERLINKS, TD_WARNING_ICON,
    };

    let copy = prompt_copy();
    let buttons = [
        TASKDIALOG_BUTTON {
            nButtonID: BTN_DOWNLOAD,
            pszButtonText: copy.download,
        },
        TASKDIALOG_BUTTON {
            nButtonID: BTN_CANCEL,
            pszButtonText: copy.cancel,
        },
    ];

    let mut config = TASKDIALOGCONFIG::default();
    config.cbSize = std::mem::size_of::<TASKDIALOGCONFIG>() as u32;
    config.dwFlags = TDF_ALLOW_DIALOG_CANCELLATION | TDF_ENABLE_HYPERLINKS;
    config.pszWindowTitle = copy.window_title;
    config.Anonymous1 = TASKDIALOGCONFIG_0 {
        pszMainIcon: TD_WARNING_ICON,
    };
    // Leave pszMainInstruction null — no large blue heading.
    config.pszContent = copy.content;
    config.cButtons = buttons.len() as u32;
    config.pButtons = buttons.as_ptr();
    config.nDefaultButton = BTN_DOWNLOAD;
    config.pfCallback = Some(task_dialog_callback);

    let mut clicked = 0i32;
    let ok = unsafe { TaskDialogIndirect(&config, Some(&mut clicked), None, None) }.is_ok();
    if !ok {
        return false;
    }
    if clicked == BTN_DOWNLOAD {
        open_download_page();
    }
    true
}

unsafe extern "system" fn task_dialog_callback(
    _hwnd: windows::Win32::Foundation::HWND,
    msg: windows::Win32::UI::Controls::TASKDIALOG_NOTIFICATIONS,
    _wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
    _ref_data: isize,
) -> windows::core::HRESULT {
    use windows::Win32::UI::Controls::TDN_HYPERLINK_CLICKED;

    if msg == TDN_HYPERLINK_CLICKED {
        let url = PCWSTR(lparam.0 as *const u16);
        if let Ok(s) = unsafe { url.to_string() } {
            if !s.is_empty() {
                open_url(&s);
            }
        }
    }
    windows::core::HRESULT(0)
}

fn show_message_box_fallback() {
    use windows::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, IDOK, MB_ICONWARNING, MB_OKCANCEL, MB_SETFOREGROUND, MB_TOPMOST,
    };

    let copy = prompt_copy();
    let result = unsafe {
        MessageBoxW(
            None,
            copy.content_plain,
            copy.window_title,
            MB_OKCANCEL | MB_ICONWARNING | MB_SETFOREGROUND | MB_TOPMOST,
        )
    };
    if result == IDOK {
        open_download_page();
    }
}

fn open_download_page() {
    open_url(DOWNLOAD_URL);
}

fn open_url(url: &str) {
    let _ = std::process::Command::new("cmd")
        .args(["/C", "start", "", url])
        .spawn();
}
