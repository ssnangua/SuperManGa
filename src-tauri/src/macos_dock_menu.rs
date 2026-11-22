//! macOS Dock context menu: "Open New Window" → spawn a secondary reader process.
//!
//! Tauri/tao do not expose Dock menus. AppKit asks the app delegate for
//! `applicationDockMenu:`; we add that selector onto tao's existing
//! `TaoAppDelegateParent` class (additive, no second delegate).

use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::OnceLock;

use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Imp, NSObject, Sel};
use objc2::{define_class, msg_send, sel, ClassType, MainThreadMarker};
use objc2_app_kit::{NSMenu, NSMenuItem};
use objc2_foundation::NSString;

static INSTALLED: AtomicBool = AtomicBool::new(false);
static DOCK_MENU: AtomicPtr<AnyObject> = AtomicPtr::new(std::ptr::null_mut());
/// Keeps the menu-item target alive for the process lifetime.
static TARGET: OnceLock<Retained<DockMenuTarget>> = OnceLock::new();

define_class!(
    // SAFETY: NSObject has no subclassing requirements; DockMenuTarget has no Drop.
    #[unsafe(super(NSObject))]
    #[name = "SuperManGaDockMenuTarget"]
    struct DockMenuTarget;

    impl DockMenuTarget {
        #[unsafe(method(openNewWindow:))]
        fn open_new_window(&self, _sender: Option<&AnyObject>) {
            let _ = crate::instance::spawn_secondary_instance();
        }
    }
);

/// Install the Dock menu. Safe to call more than once; no-ops after success or permanent failure.
pub fn install() {
    if INSTALLED.load(Ordering::Relaxed) {
        return;
    }
    match install_inner() {
        Ok(()) => {
            INSTALLED.store(true, Ordering::Relaxed);
            crate::app_log::info("instance", "macOS Dock menu: Open New Window installed");
        }
        Err(InstallError::Retry(err)) => {
            crate::app_log::warn(
                "instance",
                format!("macOS Dock menu not ready yet: {err}; will retry"),
            );
        }
        Err(InstallError::Fatal(err)) => {
            INSTALLED.store(true, Ordering::Relaxed);
            crate::app_log::warn("instance", format!("macOS Dock menu install failed: {err}"));
        }
    }
}

enum InstallError {
    /// Delegate class not registered yet — try again on Ready.
    Retry(String),
    Fatal(String),
}

fn install_inner() -> Result<(), InstallError> {
    let mtm = MainThreadMarker::new().ok_or_else(|| {
        InstallError::Retry("Dock menu must be installed on the main thread".into())
    })?;

    let Some(cls) = AnyClass::get(c"TaoAppDelegateParent") else {
        return Err(InstallError::Retry(
            "TaoAppDelegateParent not registered yet".into(),
        ));
    };

    let dock_sel = sel!(applicationDockMenu:);
    if cls.instance_method(dock_sel).is_some() {
        // Another plugin already owns the selector — do not replace it.
        return Err(InstallError::Fatal(
            "applicationDockMenu: already present on TaoAppDelegateParent".into(),
        ));
    }

    let target: Retained<DockMenuTarget> = unsafe { msg_send![DockMenuTarget::class(), new] };
    let _ = TARGET.set(target.clone());

    let title = NSString::from_str(crate::instance::open_new_window_title());
    let empty = NSString::from_str("");
    let menu = NSMenu::initWithTitle(mtm.alloc::<NSMenu>(), &empty);
    let item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            mtm.alloc::<NSMenuItem>(),
            &title,
            Some(sel!(openNewWindow:)),
            &empty,
        )
    };
    unsafe {
        item.setTarget(Some(&*((&*target as *const DockMenuTarget).cast::<AnyObject>())));
    }
    menu.addItem(&item);

    // Leak the retained menu for the process lifetime; AppKit borrows it on each Dock click.
    let menu_ptr = Retained::into_raw(menu).cast::<AnyObject>();
    DOCK_MENU.store(menu_ptr, Ordering::SeqCst);

    // SAFETY: Signature matches `- (NSMenu *)applicationDockMenu:(NSApplication *)sender`.
    // Encoding `@@:@` = id return, self, _cmd, id arg.
    let imp: Imp = unsafe {
        std::mem::transmute(
            application_dock_menu
                as unsafe extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject) -> *mut AnyObject,
        )
    };
    let added = unsafe {
        objc2::ffi::class_addMethod(
            cls as *const AnyClass as *mut AnyClass,
            dock_sel,
            imp,
            c"@@:@".as_ptr(),
        )
    };
    if !added.as_bool() {
        return Err(InstallError::Fatal(
            "class_addMethod(applicationDockMenu:) failed".into(),
        ));
    }
    Ok(())
}

unsafe extern "C-unwind" fn application_dock_menu(
    _this: *mut AnyObject,
    _cmd: Sel,
    _sender: *mut AnyObject,
) -> *mut AnyObject {
    DOCK_MENU.load(Ordering::Relaxed)
}
