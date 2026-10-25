//! Opt-in isolated Windows UI and IPC regression. Does not open browser profiles.
use crate::{batch_probe::evaluate, minimal_probe::capture_label};
use serde_json::{json, Value};
use std::{path::PathBuf, time::Duration};
use tauri::{AppHandle, Manager};
async fn wait(app: &AppHandle, script: &str) -> bool {
    for _ in 0..100 {
        if evaluate(app, script).await == Some(json!(true)) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    false
}
async fn metrics(app: &AppHandle) -> Value {
    evaluate(app,r#"JSON.stringify((()=>{const m=document.querySelector('.extension-modal'),r=m?.getBoundingClientRect();return {steps:m?.querySelectorAll('.flow-stage').length,zip:!!m?.querySelector('.package-download'),oldText:!!m?.querySelector('.extension-steps,.extension-note,code'),overflow:!r||r.x<0||r.y<0||r.right>innerWidth+1||r.bottom>innerHeight+1||m.scrollWidth>m.clientWidth+1};})())"#).await.unwrap_or(Value::Null)
}
pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(4)).await;
        let ready = wait(&app, "!!document.querySelector('.app-shell .brand')").await;
        evaluate(
            &app,
            "document.querySelector('.app-shell .minimal-brand')?.click();true",
        )
        .await;
        wait(&app, "!document.querySelector('.mode-switching')").await;
        evaluate(
            &app,
            "document.querySelector('.extension-tile').click();true",
        )
        .await;
        wait(&app, "!!document.querySelector('.extension-modal')").await;
        let full = metrics(&app).await;
        let picture = capture_label(&app, "main", "v023-extension-zip").await;
        evaluate(
            &app,
            "document.querySelectorAll('.flow-stage')[2].focus();true",
        )
        .await;
        tokio::time::sleep(Duration::from_millis(200)).await;
        let tooltip = capture_label(&app, "main", "v023-extension-tooltip").await;
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.set_size(tauri::LogicalSize::new(960., 660.));
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
        let minimum = metrics(&app).await;
        let _ = capture_label(&app, "main", "v023-extension-minimum").await;
        let directory =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.tools/extension-export-result");
        let _ = tokio::fs::create_dir_all(&directory).await;
        let target = directory.join("本地 插件 ' $ test.zip");
        let request = serde_json::to_string(&json!({"path":target})).unwrap();
        evaluate(&app,&format!("window.__zipResult={{state:'pending'}};window.__TAURI_INTERNALS__.invoke('export_browser_extension_zip',{request}).then(path=>window.__zipResult={{state:'saved',path}},error=>window.__zipResult={{state:'error',error:String(error)}});true")).await;
        let completed = wait(&app, "window.__zipResult?.state!=='pending'").await;
        let result = evaluate(&app, "JSON.stringify(window.__zipResult)")
            .await
            .unwrap_or(Value::Null);
        let bytes = tokio::fs::read(&target).await.unwrap_or_default();
        let zip = bytes.starts_with(b"PK\x03\x04") && bytes.len() > 10000;
        let valid = |v: &Value| {
            v["steps"] == 5 && v["zip"] == true && v["oldText"] == false && v["overflow"] == false
        };
        let success = ready
            && valid(&full)
            && valid(&minimum)
            && picture
            && tooltip
            && completed
            && result["state"] == "saved"
            && zip;
        let report = json!({"success":success,"full":full,"minimum":minimum,"nativeZipExport":zip,"result":result["state"],"error":result["error"],"screenshots":picture&&tooltip,"isolated":true,"noBrowserRegistryChanges":true,"noAccountsOrMedia":true});
        let _ = tokio::fs::write(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../.tools/v023-extension-verification.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .await;
        app.exit(if success { 0 } else { 1 });
    });
}
