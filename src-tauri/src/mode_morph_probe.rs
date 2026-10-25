//! Opt-in animation QA in an isolated app/profile. No accounts or downloads.
use crate::{batch_probe::evaluate, minimal_probe::capture_label};
use serde_json::{json, Value};
use std::{path::PathBuf, time::Duration};
use tauri::{AppHandle, Manager};

async fn wait(app: &AppHandle, script: &str) -> bool {
    for _ in 0..60 {
        if evaluate(app, script).await == Some(json!(true)) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    false
}
async fn switch(app: &AppHandle, forward: bool, pictures: bool) -> Value {
    evaluate(app, r#"(()=>{window.__modeFrames=[];window.__modeDone=false;let began=false;function read(){const n=document.querySelector('.mode-morph-bar');if(n){began=true;const r=n.getBoundingClientRect();window.__modeFrames.push({x:r.x,y:r.y,w:r.width,h:r.height,radius:parseFloat(getComputedStyle(n).borderRadius),clip:getComputedStyle(document.querySelector('.mode-reveal-layer')).clipPath,path:n.querySelector('.mode-icon-shaft')?.getAttribute('d')});}if(began&&!n)window.__modeDone=true;else requestAnimationFrame(read)}requestAnimationFrame(read);return true})()"#).await;
    let selector = if forward {
        ".standard-mode .brand"
    } else {
        ".minimal-brand"
    };
    evaluate(app,&format!("document.querySelector('.app-shell {selector}').dispatchEvent(new MouseEvent('click',{{bubbles:true,detail:1,clientX:60,clientY:48}}));true")).await;
    if pictures {
        tokio::time::sleep(Duration::from_millis(120)).await;
        let _ = capture_label(
            app,
            "main",
            if forward {
                "v023-enter-early"
            } else {
                "v023-return-early"
            },
        )
        .await;
        tokio::time::sleep(Duration::from_millis(130)).await;
        let _ = capture_label(
            app,
            "main",
            if forward {
                "v023-enter-middle"
            } else {
                "v023-return-middle"
            },
        )
        .await;
    }
    wait(
        app,
        "window.__modeDone===true&&!document.querySelector('.mode-switching')",
    )
    .await;
    evaluate(app,r#"JSON.stringify((()=>{const a=window.__modeFrames||[],f=a[0],l=a.at(-1);return {frames:a.length,travels:!!f&&!!l&&Math.abs(f.y-l.y)>12,heights:!!f&&!!l&&Math.abs(f.h-l.h)>12,iconMorph:new Set(a.map(n=>n.path)).size>5,clickOrigin:a.length>0&&a.every(n=>n.clip.includes('at 60px 48px')),cleanup:!document.querySelector('.mode-morph-bar,.mode-switch-capture,.mode-reveal-layer'),focused:document.activeElement?.tagName==='INPUT',overflow:document.documentElement.scrollWidth>innerWidth}})())"#).await.unwrap_or(Value::Null)
}
pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(4)).await;
        let ready = wait(&app, "!!document.querySelector('.app-shell .brand')").await;
        evaluate(&app,"if(document.querySelector('.app-shell .minimal-brand'))document.querySelector('.app-shell .minimal-brand').click();true").await;
        wait(&app, "!document.querySelector('.mode-switching')").await;
        let forward = switch(&app, true, true).await;
        let _ = capture_label(&app, "main", "v023-minimal").await;
        let reverse = switch(&app, false, true).await;
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.set_size(tauri::LogicalSize::new(960., 660.));
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
        let minimum = switch(&app, true, false).await;
        let _ = capture_label(&app, "main", "v023-minimum").await;
        let valid = |v: &Value| {
            v["frames"].as_u64().unwrap_or(0) >= 8
                && [
                    "travels",
                    "heights",
                    "iconMorph",
                    "clickOrigin",
                    "cleanup",
                    "focused",
                ]
                .iter()
                .all(|key| v[key] == true)
                && v["overflow"] == false
        };
        let success = ready && valid(&forward) && valid(&reverse) && valid(&minimum);
        let report = json!({"success":success,"forward":forward,"reverse":reverse,"minimum":minimum,"isolated":true,"noMediaOrAccounts":true});
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.tools/v023-native-mode.json");
        let _ = tokio::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).await;
        app.exit(if success { 0 } else { 1 });
    });
}
