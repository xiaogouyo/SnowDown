use crate::{
    updater::{Phase, UpdateStatus},
    AppState,
};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};

#[derive(Default)]
pub struct Notice {
    shown: Mutex<String>,
}
pub fn create(app: &AppHandle) -> tauri::Result<()> {
    WebviewWindowBuilder::new(
        app,
        "update-notice",
        WebviewUrl::App("index.html?view=update-notice".into()),
    )
    .title("SnowDown 更新已就绪")
    .inner_size(386., 236.)
    .resizable(false)
    .decorations(false)
    .transparent(true)
    .shadow(false)
    .skip_taskbar(true)
    .always_on_top(true)
    .visible(false)
    .focused(false)
    .build()?;
    Ok(())
}
pub fn dismiss(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("update-notice") {
        let _ = window.hide();
    }
}
pub fn sync(app: &AppHandle, status: &UpdateStatus) {
    if status.phase != Phase::Ready {
        dismiss(app);
        return;
    }
    let Some(release) = &status.release else {
        return;
    };
    let state = app.state::<Arc<AppState>>();
    let mut shown = state.update_notice.shown.lock().unwrap();
    if *shown == release.version {
        return;
    }
    let Some(window) = app.get_webview_window("update-notice") else {
        return;
    };
    let monitor = app
        .get_webview_window("main")
        .and_then(|main| main.current_monitor().ok().flatten())
        .or_else(|| window.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        return;
    };
    let scale = monitor.scale_factor();
    let area = monitor.work_area();
    let width = (386. * scale).round() as u32;
    let height = (236. * scale).round() as u32;
    let margin = (16. * scale).round() as i32;
    let x = (area.position.x as i64 + area.size.width as i64 - width as i64 - margin as i64)
        .max(area.position.x as i64) as i32;
    let y = (area.position.y as i64 + area.size.height as i64 - height as i64 - margin as i64)
        .max(area.position.y as i64) as i32;
    if window.set_position(PhysicalPosition::new(x, y)).is_ok()
        && window.set_size(PhysicalSize::new(width, height)).is_ok()
        && window.show().is_ok()
    {
        *shown = release.version.clone();
    }
}
