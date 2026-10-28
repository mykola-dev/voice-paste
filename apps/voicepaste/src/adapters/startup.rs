//! Current-user Windows sign-in registration. Never touches legacy's Run value.
use std::{
    os::windows::ffi::OsStrExt,
    path::Path,
    ptr::{null, null_mut},
};
use windows_sys::Win32::{Foundation::*, System::Registry::*};

const VALUE: windows_sys::core::PCWSTR = windows_sys::w!("VoicePasteRust");
struct Key(HKEY);
impl Drop for Key {
    fn drop(&mut self) {
        // SAFETY: this adapter exclusively owns this open registry key.
        unsafe {
            RegCloseKey(self.0);
        }
    }
}

pub struct StartupChange {
    key: Key,
    previous: Option<(u32, Vec<u8>)>,
}

fn command(exe: &Path) -> Result<Vec<u8>, String> {
    if !exe.is_absolute()
        || exe
            .as_os_str()
            .encode_wide()
            .any(|c| c == 0 || c == b'"' as u16)
    {
        return Err("Startup requires an absolute executable path without quotes or NUL".into());
    }
    Ok(std::iter::once(b'"' as u16)
        .chain(exe.as_os_str().encode_wide())
        .chain([b'"' as u16, 0])
        .flat_map(u16::to_le_bytes)
        .collect())
}

fn check(status: u32) -> Result<(), String> {
    if status == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(format!(
            "Windows startup registration: {}",
            std::io::Error::from_raw_os_error(status as i32)
        ))
    }
}

impl StartupChange {
    pub fn apply(previous_enabled: bool, enabled: bool) -> Result<Option<Self>, String> {
        // Ordinary saves/default startup must not touch the user's registry.
        if !previous_enabled && !enabled {
            return Ok(None);
        }
        let bytes = if enabled {
            Some(command(
                &std::env::current_exe().map_err(|e| e.to_string())?,
            )?)
        } else {
            None
        };
        // SAFETY: all UTF-16 literals/buffers are NUL terminated and live for the
        // synchronous call; registry handles are confined to this adapter.
        unsafe {
            let mut handle = null_mut();
            check(RegCreateKeyExW(
                HKEY_CURRENT_USER,
                windows_sys::w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run"),
                0,
                null(),
                REG_OPTION_NON_VOLATILE,
                KEY_QUERY_VALUE | KEY_SET_VALUE,
                null(),
                &mut handle,
                null_mut(),
            ))?;
            let key = Key(handle);
            let mut kind = 0;
            let mut len = 0;
            let status = RegQueryValueExW(key.0, VALUE, null(), &mut kind, null_mut(), &mut len);
            let previous = if status == ERROR_FILE_NOT_FOUND {
                None
            } else {
                check(status)?;
                if len > 65536 {
                    return Err("Startup registry value is unexpectedly large".into());
                }
                let mut data = vec![0; len as usize];
                check(RegQueryValueExW(
                    key.0,
                    VALUE,
                    null(),
                    &mut kind,
                    data.as_mut_ptr(),
                    &mut len,
                ))?;
                data.truncate(len as usize);
                Some((kind, data))
            };
            if let Some(data) = bytes {
                check(RegSetValueExW(
                    key.0,
                    VALUE,
                    0,
                    REG_SZ,
                    data.as_ptr(),
                    data.len() as u32,
                ))?;
            } else {
                let status = RegDeleteValueW(key.0, VALUE);
                if status != ERROR_FILE_NOT_FOUND {
                    check(status)?;
                }
            }
            Ok(Some(Self { key, previous }))
        }
    }

    pub fn rollback(self) -> Result<(), String> {
        // SAFETY: restores the exact prior type/bytes, not a reconstructed path.
        unsafe {
            match &self.previous {
                Some((kind, data)) => check(RegSetValueExW(
                    self.key.0,
                    VALUE,
                    0,
                    *kind,
                    data.as_ptr(),
                    data.len() as u32,
                )),
                None => {
                    let status = RegDeleteValueW(self.key.0, VALUE);
                    if status == ERROR_FILE_NOT_FOUND {
                        Ok(())
                    } else {
                        check(status)
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn startup_command_quotes_unicode_paths_and_never_uses_a_shell() {
        let path = std::env::temp_dir()
            .join("Мої програми")
            .join("VoicePaste.exe");
        let bytes = command(&path).unwrap();
        let units: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|v| u16::from_le_bytes([v[0], v[1]]))
            .collect();
        assert_eq!(
            String::from_utf16(&units[..units.len() - 1]).unwrap(),
            format!("\"{}\"", path.display())
        );
        assert_eq!(units.last(), Some(&0));
        assert!(command(Path::new("VoicePaste.exe")).is_err());
        assert!(command(&std::env::temp_dir().join("bad\"path.exe")).is_err());
    }
    #[test]
    fn disabled_default_never_opens_or_changes_registry() {
        assert!(StartupChange::apply(false, false).unwrap().is_none());
    }
}
