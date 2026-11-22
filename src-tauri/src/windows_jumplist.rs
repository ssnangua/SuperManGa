//! Windows taskbar Jump List task: "Open New Window" → launch another process.
//!
//! Registered at runtime via ICustomDestinationList (works for portable builds too).

use windows::core::{Interface, HSTRING};
use windows::Win32::Storage::EnhancedStorage::PKEY_Title;
use windows::Win32::System::Com::StructuredStorage::InitPropVariantFromStringAsVector;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Shell::Common::{IObjectArray, IObjectCollection};
use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
use windows::Win32::UI::Shell::{
    DestinationList, EnumerableObjectCollection, ICustomDestinationList, IShellLinkW, ShellLink,
};

const NEW_WINDOW_ARGS: &str = "--new-window";

pub fn install() {
    if let Err(err) = install_inner() {
        crate::app_log::warn("instance", format!("Jump List install failed: {err}"));
    } else {
        crate::app_log::info("instance", "Jump List: Open New Window installed");
    }
}

fn install_inner() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|err| err.to_string())?;
    let exe = exe.to_string_lossy();
    let title = crate::instance::open_new_window_title();

    unsafe {
        // Tauri may already have initialized COM; S_FALSE / already-init is fine.
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        // Do not SetAppID: keep the process default identity so the list attaches to
        // this taskbar button (portable path-based AUMID).
        let dest: ICustomDestinationList =
            CoCreateInstance(&DestinationList, None, CLSCTX_INPROC_SERVER)
                .map_err(|err| format!("DestinationList: {err}"))?;

        let mut max_slots = 0u32;
        let _removed: IObjectArray = dest
            .BeginList(&mut max_slots)
            .map_err(|err| format!("BeginList: {err}"))?;

        let collection: IObjectCollection =
            CoCreateInstance(&EnumerableObjectCollection, None, CLSCTX_INPROC_SERVER)
                .map_err(|err| format!("EnumerableObjectCollection: {err}"))?;

        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)
            .map_err(|err| format!("ShellLink: {err}"))?;
        link.SetPath(&HSTRING::from(exe.as_ref()))
            .map_err(|err| format!("SetPath: {err}"))?;
        // Jump List user tasks require arguments (MSDN).
        link.SetArguments(&HSTRING::from(NEW_WINDOW_ARGS))
            .map_err(|err| format!("SetArguments: {err}"))?;
        link.SetIconLocation(&HSTRING::from(exe.as_ref()), 0)
            .map_err(|err| format!("SetIconLocation: {err}"))?;
        link.SetDescription(&HSTRING::from(title))
            .map_err(|err| format!("SetDescription: {err}"))?;

        let store: IPropertyStore = link
            .cast()
            .map_err(|err| format!("IPropertyStore: {err}"))?;
        let variant = InitPropVariantFromStringAsVector(&HSTRING::from(title))
            .map_err(|err| format!("InitPropVariantFromStringAsVector: {err}"))?;
        store
            .SetValue(&PKEY_Title, &variant)
            .map_err(|err| format!("SetValue PKEY_Title: {err}"))?;
        store
            .Commit()
            .map_err(|err| format!("IPropertyStore::Commit: {err}"))?;

        collection
            .AddObject(&link)
            .map_err(|err| format!("AddObject: {err}"))?;
        let array: IObjectArray = collection
            .cast()
            .map_err(|err| format!("IObjectArray: {err}"))?;
        dest.AddUserTasks(&array)
            .map_err(|err| format!("AddUserTasks: {err}"))?;
        dest.CommitList()
            .map_err(|err| format!("CommitList: {err}"))?;
    }
    Ok(())
}
