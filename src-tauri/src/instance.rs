//! Detect the first live reader process so only it auto-resumes the last comic.
//! Secondary launches (Start Menu / Linux app menu / macOS Dock "Open New Window")
//! skip auto-resume unless given an explicit file path.
//!
//! Dock / taskbar left-clicks keep OS default focus behavior.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

static CLAIMED: OnceLock<bool> = OnceLock::new();
static IS_PRIMARY: AtomicBool = AtomicBool::new(true);

/// Claim primary-instance status once per process. Later calls return the same result.
pub fn claim_primary_instance() -> bool {
    *CLAIMED.get_or_init(|| {
        let primary = try_claim();
        IS_PRIMARY.store(primary, Ordering::Relaxed);
        primary
    })
}

pub fn is_primary_instance() -> bool {
    // Ensure claim runs even if setup forgot to call it.
    claim_primary_instance();
    IS_PRIMARY.load(Ordering::Relaxed)
}

/// Localized label for Dock / Jump List / .desktop “open new window” actions.
pub fn open_new_window_title() -> &'static str {
    let tag = ui_lang_tag();
    if tag.starts_with("zh-hant")
        || tag.starts_with("zh-tw")
        || tag.starts_with("zh-hk")
        || tag.starts_with("zh-mo")
    {
        "打開新視窗"
    } else if tag.starts_with("zh") {
        "打开新窗口"
    } else if tag.starts_with("ja") {
        "新しいウインドウを開く"
    } else if tag.starts_with("ko") {
        "새 창 열기"
    } else {
        "Open New Window"
    }
}

fn ui_lang_tag() -> String {
    #[cfg(windows)]
    {
        use windows::Win32::Globalization::GetUserDefaultLocaleName;
        let mut buf = [0u16; 85];
        let n = unsafe { GetUserDefaultLocaleName(&mut buf) };
        if n > 1 {
            return String::from_utf16_lossy(&buf[..(n as usize - 1)]).to_ascii_lowercase();
        }
    }
    #[cfg(target_os = "macos")]
    {
        use objc2_foundation::NSLocale;
        if let Some(lang) = NSLocale::preferredLanguages().firstObject() {
            return lang.to_string().to_ascii_lowercase();
        }
    }
    #[cfg(target_os = "linux")]
    {
        for key in ["LC_ALL", "LC_MESSAGES", "LANG"] {
            if let Ok(value) = std::env::var(key) {
                let tag = value
                    .split('.')
                    .next()
                    .unwrap_or(&value)
                    .replace('_', "-")
                    .to_ascii_lowercase();
                if !tag.is_empty() && tag != "c" {
                    return tag;
                }
            }
        }
    }
    String::new()
}

/// Start another reader process (empty window).
/// Used by Dock / Jump List / File menu “Open New Window”.
pub fn spawn_secondary_instance() -> Result<(), String> {
    spawn_secondary_inner().inspect_err(|err| {
        crate::app_log::warn("instance", format!("spawn secondary failed: {err}"));
    })
}

fn spawn_secondary_inner() -> Result<(), String> {
    use std::process::{Command, Stdio};

    let exe = std::env::current_exe().map_err(|err| err.to_string())?;

    #[cfg(target_os = "macos")]
    {
        if let Some(app_bundle) = exe.ancestors().find(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("app"))
        }) {
            Command::new("open")
                .args(["-n"])
                .arg(app_bundle)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|err| err.to_string())?;
            return Ok(());
        }
    }

    Command::new(&exe)
        .arg("--new-window")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|err| err.to_string())?;
    Ok(())
}

#[cfg(windows)]
fn try_claim() -> bool {
    use std::sync::atomic::AtomicPtr;
    use std::sync::atomic::Ordering::SeqCst;
    use windows::core::w;
    use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS};
    use windows::Win32::System::Threading::CreateMutexW;

    // Keep the owning handle alive for the process lifetime (OS releases on exit).
    static HOLD: AtomicPtr<std::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());

    // Local\ — per user session; allows another user session its own primary.
    let created = unsafe { CreateMutexW(None, true, w!("Local\\SuperManGa.PrimaryReader")) };
    match created {
        Ok(handle) => {
            let already = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
            if already {
                unsafe {
                    let _ = CloseHandle(handle);
                }
                false
            } else {
                HOLD.store(handle.0, SeqCst);
                true
            }
        }
        Err(_) => true,
    }
}

#[cfg(unix)]
fn try_claim() -> bool {
    use std::fs::OpenOptions;
    use std::os::fd::AsRawFd;

    // Keep the lock file open for the process lifetime (flock released on close/exit).
    static HOLD: OnceLock<std::fs::File> = OnceLock::new();

    let path = std::env::temp_dir().join("supermanga-primary-reader.lock");
    let file = match OpenOptions::new().create(true).truncate(false).write(true).open(&path) {
        Ok(file) => file,
        Err(_) => return true,
    };
    let locked = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0;
    if locked {
        let _ = HOLD.set(file);
        true
    } else {
        false
    }
}

#[cfg(not(any(windows, unix)))]
fn try_claim() -> bool {
    true
}
