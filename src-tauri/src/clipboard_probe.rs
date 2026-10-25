//! Opt-in QA: inject public recognized links, without changing the user's clipboard.
use crate::{
    batch_probe::{evaluate, evaluate_window},
    clipboard_watch, AppState,
};
use serde_json::{json, Value};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tauri::{AppHandle, Manager, PhysicalPosition};

async fn ui(app: &AppHandle, script: &str) -> bool {
    for _ in 0..30 {
        if evaluate(app, script).await == Some(json!(true)) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    false
}

pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(6)).await;
        let state = app.state::<Arc<AppState>>();
        let main = app.get_webview_window("main").unwrap();
        {
            let mut settings = state.settings.lock().unwrap();
            settings.clipboard_enabled = true;
            settings.clipboard_auto_parse = true;
        }
        state.clipboard.configure(true, true);
        state.emit(&app);
        tokio::time::sleep(Duration::from_millis(400)).await;
        let _ = main.hide();
        tokio::time::sleep(Duration::from_millis(400)).await;
        state.clipboard.detect(
            crate::sites::recognize("https://www.bilibili.com/video/BV17x411w7KC?p=10").unwrap(),
            1,
        );
        clipboard_watch::emit(&app, &state);
        let mut hidden_ready = false;
        let mut error = None;
        for _ in 0..160 {
            if let Some(capture) = state.clipboard.snapshot() {
                if capture.phase == clipboard_watch::Phase::Ready {
                    hidden_ready = main.is_visible().ok() == Some(false);
                    break;
                }
                if capture.phase == clipboard_watch::Phase::Error {
                    error = capture.error;
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
        let _ = main.show();
        let _ = main.set_focus();
        let restored = ui(
            &app,
            "!!document.querySelector('.video-modal .video-summary h3')?.textContent.trim()",
        )
        .await;
        let _ = evaluate(
            &app,
            "document.querySelector('[aria-label=\"关闭视频选择\"]')?.click();true",
        )
        .await;
        tokio::time::sleep(Duration::from_millis(400)).await;
        let source =
            crate::sites::recognize("https://www.bilibili.com/video/BV17x411w7KC?p=9").unwrap();
        state.clipboard.detect(source, 2);
        clipboard_watch::emit(&app, &state);
        let second = ui(
            &app,
            "!!document.querySelector('.video-modal .video-summary h3')?.textContent.trim()",
        )
        .await;
        let no_tasks = state.tasks.lock().unwrap().is_empty();
        let _ = evaluate(
            &app,
            "document.querySelector('[aria-label=\"关闭视频选择\"]')?.click();true",
        )
        .await;
        let monitor = main.primary_monitor().unwrap().unwrap();
        let area = monitor.work_area();
        let cursor = PhysicalPosition::new(
            (area.position.x as i64 + area.size.width as i64 - 20) as f64,
            (area.position.y as i64 + area.size.height as i64 - 5) as f64,
        );
        let shown = crate::tray_menu::show(&app, cursor).is_ok();
        tokio::time::sleep(Duration::from_millis(600)).await;
        let layout = evaluate_window(&app,"tray-menu",r#"JSON.stringify((()=>{const c=document.querySelector('.tray-card'),b=document.querySelectorAll('[role=menuitem]');return {items:b.length,brand:c?.textContent.includes('SnowDown'),version:c?.textContent.includes('v0.2.2'),noOverflow:document.documentElement.scrollWidth===innerWidth&&document.documentElement.scrollHeight===innerHeight,rounded:c?parseFloat(getComputedStyle(c).borderRadius)>0:false,transparent:getComputedStyle(document.documentElement).backgroundColor==='rgba(0, 0, 0, 0)'}})())"#).await.unwrap_or(Value::Null);
        let popup = app.get_webview_window("tray-menu").unwrap();
        let point = popup.outer_position().unwrap();
        let size = popup.outer_size().unwrap();
        let clamped = point.x >= area.position.x
            && point.y >= area.position.y
            && point.x as i64 + size.width as i64
                <= area.position.x as i64 + area.size.width as i64
            && point.y as i64 + size.height as i64
                <= area.position.y as i64 + area.size.height as i64;
        let _ = evaluate_window(
            &app,
            "tray-menu",
            "document.querySelector('[role=menuitem]').click();true",
        )
        .await;
        tokio::time::sleep(Duration::from_millis(500)).await;
        let open_action =
            popup.is_visible().ok() == Some(false) && main.is_visible().ok() == Some(true);
        let _ = crate::tray_menu::show(&app, cursor);
        tokio::time::sleep(Duration::from_millis(200)).await;
        let _ = evaluate_window(&app,"tray-menu","document.activeElement.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true}));true").await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        let escape = popup.is_visible().ok() == Some(false);
        let _ = crate::tray_menu::show(&app, cursor);
        tokio::time::sleep(Duration::from_millis(200)).await;
        let _ = main.set_focus();
        tokio::time::sleep(Duration::from_millis(300)).await;
        let blur = popup.is_visible().ok() == Some(false);
        let success = hidden_ready
            && restored
            && second
            && no_tasks
            && shown
            && clamped
            && open_action
            && escape
            && blur
            && layout["items"] == 3
            && layout["noOverflow"] == true
            && layout["rounded"] == true
            && layout["transparent"] == true;
        let report = json!({"success":success,"hiddenNativeParse":hidden_ready,"restoredResult":restored,"nextCopiedLink":second,"noAutomaticDownloads":no_tasks,"trayShown":shown,"clamped":clamped,"openAction":open_action,"escapeCloses":escape,"blurCloses":blur,"layout":layout,"error":error,"systemClipboardModified":false});
        let _ = tokio::fs::write(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../.tools/v018-background-ui-verification.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .await;
        if std::env::var_os("SNOWDOWN_TRAY_PREVIEW").is_some() {
            let _ = crate::tray_menu::show(&app, cursor);
        } else {
            app.exit(0);
        }
    });
}
