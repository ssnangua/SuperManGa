//! Detect the first live reader process so only it auto-resumes the last comic.

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

#[cfg(not(windows))]
fn try_claim() -> bool {
    // No multi-process reader workflow on other platforms yet — treat each as primary.
    true
}
