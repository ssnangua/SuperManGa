use serde::Serialize;

pub const ARCHIVE_EXTS: &[&str] = &[
    "zip", "cbz", "rar", "cbr", "7z", "epub", "pdf", "mobi", "azw3", "azw",
];

const VERB: &str = "SuperManGa.Open";
const PROG_PREFIX: &str = "SuperManGa";
const BACKUP_SUFFIX: &str = "_SuperManGa_backup";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellIntegration {
    pub context_menu: bool,
    pub associations: Vec<String>,
    pub supported: bool,
}

#[cfg(windows)]
mod win {
    use super::*;
    use std::path::PathBuf;
    use winreg::enums::*;
    use winreg::RegKey;

    fn exe_path() -> Result<PathBuf, String> {
        std::env::current_exe().map_err(|err| err.to_string())
    }

    fn exe_quoted() -> Result<String, String> {
        Ok(format!("\"{}\"", exe_path()?.to_string_lossy()))
    }

    fn prog_id(ext: &str) -> String {
        format!("{PROG_PREFIX}.{ext}")
    }

    fn notify_shell() {
        #[link(name = "shell32")]
        unsafe extern "system" {
            fn SHChangeNotify(event: i32, flags: u32, item1: isize, item2: isize);
        }
        const SHCNE_ASSOCCHANGED: i32 = 0x0800_0000;
        const SHCNF_IDLIST: u32 = 0x0000;
        unsafe {
            SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, 0, 0);
        }
    }

    fn hkcu() -> RegKey {
        RegKey::predef(HKEY_CURRENT_USER)
    }

    fn write_verb(parent: &str, label: &str, exe: &str) -> Result<(), String> {
        let path = format!("{parent}\\shell\\{VERB}");
        let (key, _) = hkcu()
            .create_subkey(&path)
            .map_err(|err| err.to_string())?;
        key.set_value("", &label).map_err(|err| err.to_string())?;
        key.set_value("Icon", &format!("{exe},0"))
            .map_err(|err| err.to_string())?;
        let (cmd, _) = hkcu()
            .create_subkey(format!("{path}\\command"))
            .map_err(|err| err.to_string())?;
        cmd.set_value("", &format!("{exe} \"%1\""))
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    fn delete_tree(path: &str) {
        let _ = hkcu().delete_subkey_all(path);
    }

    pub fn get_shell_integration() -> ShellIntegration {
        let context_menu = hkcu()
            .open_subkey(format!(
                r"Software\Classes\SystemFileAssociations\.cbz\shell\{VERB}"
            ))
            .is_ok()
            || hkcu()
                .open_subkey(format!(r"Software\Classes\Directory\shell\{VERB}"))
                .is_ok();
        let mut associations = Vec::new();
        for ext in ARCHIVE_EXTS {
            if is_default_for(ext) {
                associations.push((*ext).to_string());
            }
        }
        ShellIntegration {
            context_menu,
            associations,
            supported: true,
        }
    }

    fn is_default_for(ext: &str) -> bool {
        let Ok(key) = hkcu().open_subkey(format!(r"Software\Classes\.{ext}")) else {
            return false;
        };
        let current: String = key.get_value("").unwrap_or_default();
        current.eq_ignore_ascii_case(&prog_id(ext))
    }

    pub fn set_context_menu(enabled: bool, label: &str) -> Result<(), String> {
        let exe = exe_quoted()?;
        if enabled {
            for ext in ARCHIVE_EXTS {
                write_verb(
                    &format!(r"Software\Classes\SystemFileAssociations\.{ext}"),
                    label,
                    &exe,
                )?;
            }
            write_verb(r"Software\Classes\Directory", label, &exe)?;
        } else {
            for ext in ARCHIVE_EXTS {
                delete_tree(&format!(
                    r"Software\Classes\SystemFileAssociations\.{ext}\shell\{VERB}"
                ));
            }
            delete_tree(&format!(r"Software\Classes\Directory\shell\{VERB}"));
        }
        notify_shell();
        Ok(())
    }

    pub fn apply_file_associations(extensions: &[String], description: &str) -> Result<(), String> {
        let exe = exe_quoted()?;
        let selected: Vec<String> = extensions
            .iter()
            .map(|ext| ext.trim().trim_start_matches('.').to_ascii_lowercase())
            .filter(|ext| ARCHIVE_EXTS.iter().any(|item| *item == ext.as_str()))
            .collect();

        for ext in ARCHIVE_EXTS {
            if selected.iter().any(|item| item == *ext) {
                associate_ext(ext, description, &exe)?;
            } else {
                disassociate_ext(ext)?;
            }
        }
        notify_shell();
        Ok(())
    }

    fn associate_ext(ext: &str, description: &str, exe: &str) -> Result<(), String> {
        let id = prog_id(ext);
        let (prog, _) = hkcu()
            .create_subkey(format!(r"Software\Classes\{id}"))
            .map_err(|err| err.to_string())?;
        prog.set_value("", &description)
            .map_err(|err| err.to_string())?;
        let (icon, _) = hkcu()
            .create_subkey(format!(r"Software\Classes\{id}\DefaultIcon"))
            .map_err(|err| err.to_string())?;
        icon.set_value("", &format!("{exe},0"))
            .map_err(|err| err.to_string())?;
        let (open, _) = hkcu()
            .create_subkey(format!(r"Software\Classes\{id}\shell\open\command"))
            .map_err(|err| err.to_string())?;
        open.set_value("", &format!("{exe} \"%1\""))
            .map_err(|err| err.to_string())?;

        let (ext_key, _) = hkcu()
            .create_subkey(format!(r"Software\Classes\.{ext}"))
            .map_err(|err| err.to_string())?;
        let previous: String = ext_key.get_value("").unwrap_or_default();
        if !previous.is_empty() && !previous.eq_ignore_ascii_case(&id) {
            let _ = ext_key.set_value(BACKUP_SUFFIX, &previous);
        }
        ext_key
            .set_value("", &id)
            .map_err(|err| err.to_string())?;
        let (with, _) = hkcu()
            .create_subkey(format!(r"Software\Classes\.{ext}\OpenWithProgids"))
            .map_err(|err| err.to_string())?;
        with.set_value(&id, &"")
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    fn disassociate_ext(ext: &str) -> Result<(), String> {
        let id = prog_id(ext);
        if let Ok(ext_key) = hkcu().open_subkey_with_flags(
            format!(r"Software\Classes\.{ext}"),
            KEY_READ | KEY_WRITE,
        ) {
            let current: String = ext_key.get_value("").unwrap_or_default();
            if current.eq_ignore_ascii_case(&id) {
                let backup: String = ext_key.get_value(BACKUP_SUFFIX).unwrap_or_default();
                if backup.is_empty() {
                    let _ = ext_key.delete_value("");
                } else {
                    let _ = ext_key.set_value("", &backup);
                }
                let _ = ext_key.delete_value(BACKUP_SUFFIX);
            }
            if let Ok(with) = hkcu().open_subkey_with_flags(
                format!(r"Software\Classes\.{ext}\OpenWithProgids"),
                KEY_READ | KEY_WRITE,
            ) {
                let _ = with.delete_value(&id);
            }
        }
        delete_tree(&format!(r"Software\Classes\{id}"));
        Ok(())
    }
}

#[cfg(windows)]
pub use win::{apply_file_associations, get_shell_integration, set_context_menu};

#[cfg(not(windows))]
pub fn get_shell_integration() -> ShellIntegration {
    ShellIntegration {
        context_menu: false,
        associations: Vec::new(),
        supported: false,
    }
}

#[cfg(not(windows))]
pub fn set_context_menu(_enabled: bool, _label: &str) -> Result<(), String> {
    Err("Windows only".into())
}

#[cfg(not(windows))]
pub fn apply_file_associations(_extensions: &[String], _description: &str) -> Result<(), String> {
    Err("Windows only".into())
}
