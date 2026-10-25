//! Native OS notifications; URLs, cookies and login tokens never enter toast text.
use std::{collections::VecDeque, sync::Mutex, time::{Duration, Instant}};
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

pub fn prepare(app: &AppHandle, directory: &std::path::Path) {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Registry::*;
        fn wide(value: &str) -> Vec<u16> { value.encode_utf16().chain(Some(0)).collect() }
        let icon = directory.join("notification-icon.png");
        let _ = std::fs::write(&icon, include_bytes!("../icons/64x64.png"));
        let path = wide(&format!("Software\\Classes\\AppUserModelId\\{}", app.config().identifier));
        // Register current-user toast branding for both installed and portable copies.
        // Do not change the user's notification enablement or quiet-hours preferences.
        unsafe {
            let mut key = std::ptr::null_mut();
            if RegCreateKeyExW(HKEY_CURRENT_USER, path.as_ptr(), 0, std::ptr::null(), 0, KEY_SET_VALUE, std::ptr::null(), &mut key, std::ptr::null_mut()) == 0 {
                for (name, value) in [("DisplayName", "SnowDown".to_string()), ("IconUri", icon.to_string_lossy().into_owned())] {
                    let name = wide(name); let value = wide(&value);
                    RegSetValueExW(key, name.as_ptr(), 0, REG_SZ, value.as_ptr().cast(), (value.len()*2) as u32);
                }
                RegCloseKey(key);
            }
        }
    }
    let app = app.clone();
    let callback = app.clone();
    let _ = app.notification().on_action(move |_| crate::show_main(&callback));
}

#[derive(Default)]
pub struct Notifications { recent: Mutex<VecDeque<(String, Instant)>> }
impl Notifications {
    pub fn show(&self, app: &AppHandle, key: String, title: &str, body: &str) {
        if !admit(&mut self.recent.lock().unwrap(), key, Instant::now()) { return; }
        let app = app.clone();
        let title = title.to_string();
        let body = safe_text(body);
        tauri::async_runtime::spawn_blocking(move || {
            if let Err(error) = app.notification().builder().title(title).body(body).show() {
                eprintln!("SnowDown system notification unavailable: {error}");
            }
        });
    }
}
fn safe_text(text: &str) -> String { text.chars().filter(|c| !c.is_control()).take(160).collect() }
fn admit(recent: &mut VecDeque<(String, Instant)>, key: String, now: Instant) -> bool {
    recent.retain(|(_, at)| now.duration_since(*at) < Duration::from_secs(10));
    if recent.iter().any(|(existing, _)| existing == &key) { return false; }
    if recent.len() >= 128 { recent.pop_front(); }
    recent.push_back((key, now));
    true
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn phases_deduplicate_independently_and_expire_without_exposing_urls() {
        let mut recent = VecDeque::new(); let now = Instant::now();
        assert!(admit(&mut recent, "start:task".into(), now));
        assert!(!admit(&mut recent, "start:task".into(), now));
        assert!(admit(&mut recent, "complete:task".into(), now));
        assert!(admit(&mut recent, "start:task".into(), now + Duration::from_secs(11)));
        assert_eq!(safe_text("视频\r\n标题\0"), "视频标题");
        assert_eq!(safe_text(&"字".repeat(1000)).chars().count(), 160);
    }
}
