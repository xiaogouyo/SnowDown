use crate::{show_main, AppState};
use serde::Deserialize;
use std::sync::Arc;
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};

pub const WIDTH: f64 = 264.;
pub const HEIGHT: f64 = 248.;

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Show,
    Folder,
    Quit,
    Dismiss,
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let window = WebviewWindowBuilder::new(
        app,
        "tray-menu",
        WebviewUrl::App("index.html?view=tray".into()),
    )
    .title("SnowDown 托盘菜单")
    .inner_size(WIDTH, HEIGHT)
    .resizable(false)
    .decorations(false)
    .transparent(true)
    .shadow(false)
    .skip_taskbar(true)
    .always_on_top(true)
    .visible(false)
    .focused(false)
    .build()?;
    let popup = window.clone();
    window.on_window_event(move |event| {
        if matches!(event, tauri::WindowEvent::Focused(false)) {
            let _ = popup.hide();
        }
    });
    Ok(())
}

fn position(
    cursor: (f64, f64),
    area: (i32, i32, u32, u32),
    size: (u32, u32),
) -> PhysicalPosition<i32> {
    let (left, top, width, height) = area;
    let right = (left as i64 + width as i64 - size.0 as i64).max(left as i64);
    let bottom = (top as i64 + height as i64 - size.1 as i64).max(top as i64);
    PhysicalPosition::new(
        (cursor.0 as i64 - size.0 as i64 + 14).clamp(left as i64, right) as i32,
        (cursor.1 as i64 - size.1 as i64 - 6).clamp(top as i64, bottom) as i32,
    )
}

pub fn show(app: &AppHandle, cursor: PhysicalPosition<f64>) -> Result<(), String> {
    let window = app
        .get_webview_window("tray-menu")
        .ok_or("托盘菜单未就绪。")?;
    let monitor = window
        .monitor_from_point(cursor.x, cursor.y)
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten())
        .ok_or("无法确定屏幕位置。")?;
    let area = monitor.work_area();
    let size = (
        (WIDTH * monitor.scale_factor()).round() as u32,
        (HEIGHT * monitor.scale_factor()).round() as u32,
    );
    let point = position(
        (cursor.x, cursor.y),
        (
            area.position.x,
            area.position.y,
            area.size.width,
            area.size.height,
        ),
        size,
    );
    window
        .set_position(point)
        .map_err(|_| "无法定位托盘菜单。")?;
    window
        .set_size(PhysicalSize::new(size.0, size.1))
        .map_err(|_| "无法调整托盘菜单。")?;
    window.show().map_err(|_| "无法显示托盘菜单。")?;
    window.set_focus().map_err(|_| "无法聚焦托盘菜单。")?;
    Ok(())
}

pub fn action(app: &AppHandle, action: Action) -> Result<(), String> {
    if matches!(&action, Action::Folder) {
        crate::open_download_directory(app.clone(), app.state::<Arc<AppState>>())?;
    }
    if let Some(window) = app.get_webview_window("tray-menu") {
        let _ = window.hide();
    }
    match action {
        Action::Show => show_main(app),
        Action::Folder => (),
        Action::Quit => crate::quit_after_checkpoint(app),
        Action::Dismiss => (),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn popup_stays_in_the_work_area_including_negative_and_scaled_monitors() {
        for area in [
            (0, 0, 1920, 1040),
            (-1920, -200, 1920, 1040),
            (0, 0, 3840, 2080),
        ] {
            for cursor in [
                (-10000., -10000.),
                (0., 0.),
                (1910., 1070.),
                (10000., 10000.),
            ] {
                let size = if area.2 > 1920 {
                    (528, 496)
                } else {
                    (264, 248)
                };
                let p = position(cursor, area, size);
                assert!(p.x >= area.0 && p.y >= area.1);
                assert!(p.x as i64 + size.0 as i64 <= area.0 as i64 + area.2 as i64);
                assert!(p.y as i64 + size.1 as i64 <= area.1 as i64 + area.3 as i64);
            }
        }
    }
}
