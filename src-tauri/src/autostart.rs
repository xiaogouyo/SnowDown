//! Per-user Windows startup. The OS entry is the source of truth; no cookies,
//! download settings or credentials are copied into the startup command.
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub enabled: bool,
    pub blocked: bool,
}

pub fn background(args: impl IntoIterator<Item = String>) -> bool {
    args.into_iter().skip(1).any(|arg| arg == "--autostart")
}

#[cfg(windows)]
mod os {
    use super::Status;
    use std::path::Path;
    use windows_sys::Win32::System::Registry::*;
    const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const APPROVED: &str =
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
    const NAME: &str = "SnowDown";
    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }
    fn error(code: u32) -> String {
        format!("无法修改开机自启动（Windows 错误 {code}），请检查系统权限。")
    }
    fn read(key: &str, kind: u32) -> Result<Option<Vec<u8>>, String> {
        let key = wide(key);
        let name = wide(NAME);
        let mut size = 0;
        let code = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                kind,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut size,
            )
        };
        if code == 2 || code == 3 {
            return Ok(None);
        }
        if code != 0 {
            return Err(error(code));
        }
        if size > 65536 {
            return Err("开机自启动配置过大。".into());
        }
        let mut bytes = vec![0u8; size as usize];
        let code = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                kind,
                std::ptr::null_mut(),
                bytes.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if code != 0 {
            return Err(error(code));
        }
        bytes.truncate(size as usize);
        Ok(Some(bytes))
    }
    fn entry() -> Result<Option<String>, String> {
        Ok(read(RUN, RRF_RT_REG_SZ)?.map(|bytes| {
            String::from_utf16_lossy(
                &bytes
                    .chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .take_while(|c| *c != 0)
                    .collect::<Vec<_>>(),
            )
        }))
    }
    pub(super) fn owned(s: &str) -> bool {
        s.strip_prefix('"')
            .and_then(|s| s.strip_suffix("\" --autostart"))
            .is_some_and(|p| {
                Path::new(p).is_absolute()
                    && Path::new(p)
                        .file_name()
                        .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("snowdown.exe"))
            })
    }
    fn command() -> Result<String, String> {
        let exe = std::env::current_exe().map_err(|_| "无法读取软件路径。")?;
        Ok(format!(
            "\"{}\" --autostart",
            exe.to_str().ok_or("软件路径无法用于自启动。")?
        ))
    }
    fn write(value: &str) -> Result<(), String> {
        let key = wide(RUN);
        let name = wide(NAME);
        let data = wide(value);
        let code = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                REG_SZ,
                data.as_ptr().cast(),
                (data.len() * 2) as u32,
            )
        };
        if code == 0 {
            Ok(())
        } else {
            Err(error(code))
        }
    }
    fn remove(key: &str) -> Result<(), String> {
        let key = wide(key);
        let name = wide(NAME);
        let code = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr()) };
        if matches!(code, 0 | 2 | 3) {
            Ok(())
        } else {
            Err(error(code))
        }
    }
    pub fn status() -> Result<Status, String> {
        let registered = entry()?.as_deref().is_some_and(owned);
        let blocked = registered
            && read(APPROVED, RRF_RT_REG_BINARY)?
                .is_some_and(|v| v.first().is_some_and(|b| !matches!(*b, 2 | 6)));
        Ok(Status {
            enabled: registered && !blocked,
            blocked,
        })
    }
    pub fn set(enabled: bool) -> Result<Status, String> {
        if let Some(value) = entry()? {
            if !owned(&value) {
                return Err("同名启动项不属于 SnowDown，请在 Windows 启动应用中检查。".into());
            }
        }
        if enabled {
            write(&command()?)?;
            remove(APPROVED)?;
        } else {
            remove(RUN)?;
        }
        status()
    }
    #[cfg(not(debug_assertions))]
    pub fn reconcile() -> Result<(), String> {
        // Preserve both opt-in and a Windows Task Manager disable decision.
        if entry()?.as_deref().is_some_and(owned) {
            write(&command()?)?;
        }
        Ok(())
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn only_migrates_our_quoted_command() {
            assert!(owned(r#""D:\软件下载\SnowDown.exe" --autostart"#));
            for s in [
                r#""D:\other.exe" --autostart"#,
                r#""D:\SnowDown.exe" --other"#,
                r#""SnowDown.exe" --autostart"#,
                r#"cmd /c SnowDown.exe --autostart"#,
            ] {
                assert!(!owned(s));
            }
        }
    }
}
#[cfg(windows)]
pub use os::{set, status};
pub fn reconcile() {
    #[cfg(all(windows, not(debug_assertions)))]
    let _ = os::reconcile();
}
#[cfg(not(windows))]
pub fn status() -> Result<Status, String> {
    Ok(Status {
        enabled: false,
        blocked: false,
    })
}
#[cfg(not(windows))]
pub fn set(_: bool) -> Result<Status, String> {
    Err("当前版本仅支持 Windows 开机自启动。".into())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn startup_flag_is_exact_and_skips_executable() {
        assert!(background(["SnowDown.exe".into(), "--autostart".into()]));
        assert!(!background(["--autostart".into()]));
        assert!(!background([
            "SnowDown.exe".into(),
            "https://site/--autostart".into()
        ]));
    }
}
