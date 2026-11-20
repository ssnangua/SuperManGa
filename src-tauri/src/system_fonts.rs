//! Enumerate installed font family names for the translation overlay picker.
//!
//! - Windows: DirectWrite (+ registry fallback). Avoid GDI `EnumFontFamiliesExW` —
//!   it contends with WebView2 on a process-wide GDI lock and freezes the UI.
//! - macOS / Linux: `font-enumeration` (Core Text / Fontconfig).

use std::collections::BTreeSet;

#[tauri::command]
pub async fn list_system_fonts() -> Result<Vec<String>, String> {
    // Dedicated OS thread (not just the blocking pool) so font I/O cannot stall
    // the async runtime that services the webview IPC loop.
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("list-system-fonts".into())
        .spawn(move || {
            let _ = tx.send(list_system_fonts_sync());
        })
        .map_err(|e| e.to_string())?;

    tauri::async_runtime::spawn_blocking(move || rx.recv().map_err(|e| e.to_string())?)
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(windows)]
fn list_system_fonts_sync() -> Result<Vec<String>, String> {
    match list_fonts_direct_write() {
        Ok(names) if !names.is_empty() => Ok(names),
        Ok(_) => list_fonts_from_registry(),
        Err(_) => list_fonts_from_registry(),
    }
}

#[cfg(windows)]
fn list_fonts_direct_write() -> Result<Vec<String>, String> {
    use windows::core::{BOOL, w};
    use windows::Win32::Graphics::DirectWrite::{
        DWriteCreateFactory, IDWriteFactory, IDWriteFontCollection, DWRITE_FACTORY_TYPE_SHARED,
    };

    unsafe {
        let factory: IDWriteFactory =
            DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).map_err(|e| e.to_string())?;
        let mut collection: Option<IDWriteFontCollection> = None;
        factory
            .GetSystemFontCollection(&mut collection, false)
            .map_err(|e| e.to_string())?;
        let collection = collection
            .ok_or_else(|| "GetSystemFontCollection returned null".to_string())?;
        let count = collection.GetFontFamilyCount();
        let mut names = BTreeSet::new();

        for i in 0..count {
            let Ok(family) = collection.GetFontFamily(i) else {
                continue;
            };
            let Ok(family_names) = family.GetFamilyNames() else {
                continue;
            };
            let name_count = family_names.GetCount();
            if name_count == 0 {
                continue;
            }

            let mut idx = 0u32;
            let mut exists = BOOL(0);
            let locale_hit = [
                w!("zh-cn"),
                w!("zh-tw"),
                w!("zh-hk"),
                w!("ja-jp"),
                w!("ko-kr"),
                w!("en-us"),
            ]
            .into_iter()
            .find_map(|locale| {
                if family_names
                    .FindLocaleName(locale, &mut idx, &mut exists)
                    .is_ok()
                    && exists.as_bool()
                {
                    Some(idx)
                } else {
                    None
                }
            });
            let idx = locale_hit.unwrap_or(0);
            if idx >= name_count {
                continue;
            }

            let Ok(len) = family_names.GetStringLength(idx) else {
                continue;
            };
            let mut buf = vec![0u16; len as usize + 1];
            if family_names.GetString(idx, &mut buf).is_err() {
                continue;
            }
            let name = String::from_utf16_lossy(&buf[..len as usize])
                .trim()
                .to_string();
            if !name.is_empty() && !name.starts_with('@') {
                names.insert(name);
            }
        }

        Ok(names.into_iter().collect())
    }
}

#[cfg(windows)]
fn list_fonts_from_registry() -> Result<Vec<String>, String> {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ};
    use winreg::RegKey;

    let mut names = BTreeSet::new();
    let paths = [
        (
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts",
        ),
        (
            HKEY_CURRENT_USER,
            r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts",
        ),
    ];

    for (hive, path) in paths {
        let Ok(root) = RegKey::predef(hive).open_subkey_with_flags(path, KEY_READ) else {
            continue;
        };
        for (name, _) in root.enum_values().filter_map(|e| e.ok()) {
            push_registry_font_name(&mut names, &name);
        }
    }

    if names.is_empty() {
        return Err("No fonts found in registry".into());
    }
    Ok(names.into_iter().collect())
}

#[cfg(windows)]
fn push_registry_font_name(names: &mut BTreeSet<String>, raw: &str) {
    let mut s = raw.trim().to_string();
    for suffix in [
        " (TrueType)",
        " (OpenType)",
        " (Type 1)",
        " (All res)",
        " (VGA res)",
        " (Plotter)",
        " (PostScript)",
        " (Vector)",
    ] {
        if let Some(stripped) = s.strip_suffix(suffix) {
            s = stripped.trim().to_string();
            break;
        }
    }
    if s.is_empty() || s.starts_with('@') {
        return;
    }
    // TTC bundles often look like "微软雅黑 & Microsoft YaHei UI".
    for part in s.split(" & ") {
        let part = part.trim();
        if !part.is_empty() && !part.starts_with('@') {
            names.insert(part.to_string());
        }
    }
}

#[cfg(not(windows))]
fn list_system_fonts_sync() -> Result<Vec<String>, String> {
    let collection =
        font_enumeration::Collection::new().map_err(|e| e.to_string())?;
    let mut names = BTreeSet::new();
    for font in collection.all() {
        let name = font.family_name.trim();
        if name.is_empty() || name.starts_with('@') {
            continue;
        }
        names.insert(name.to_string());
    }
    if names.is_empty() {
        return Err("No system fonts found".into());
    }
    Ok(names.into_iter().collect())
}
