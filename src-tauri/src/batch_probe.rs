//! Opt-in integration check of the actual Vue dialog, native IPC, queue and media pipeline.
//! SNOWDOWN_BATCH_VERIFY uses an isolated database and public Bilibili samples only.
use crate::{model::*, AppState};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::{AppHandle, Manager};

async fn evaluate(app: &AppHandle, script: &str) -> Option<Value> {
    let window = app.get_webview_window("main")?;
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let sender = Mutex::new(Some(sender));
    window
        .eval_with_callback(script, move |value| {
            if let Some(sender) = sender.lock().unwrap().take() {
                let _ = sender.send(value);
            }
        })
        .ok()?;
    let raw = tokio::time::timeout(Duration::from_secs(3), receiver)
        .await
        .ok()?
        .ok()?;
    let value: Value = serde_json::from_str(&raw).ok()?;
    if let Some(text) = value.as_str() {
        serde_json::from_str(text).ok()
    } else {
        Some(value)
    }
}

pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let state = app.state::<Arc<AppState>>();
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../.tools/batch-qa-downloads/{}", now_ms()));
        let mut settings = state.settings.lock().unwrap().clone();
        settings.download_dir = directory.to_string_lossy().into_owned();
        settings.concurrency = 2;
        settings.clipboard_enabled = false;
        let _ = state.storage.save_settings(&settings);
        *state.settings.lock().unwrap() = settings.clone();
        // Repeated runs use fresh records without touching the user's actual database.
        state.tasks.lock().unwrap().clear();
        state.emit(&app);
        for _ in 0..60 {
            if evaluate(&app, "!!document.querySelector('.new-task:not(:disabled)')").await
                == Some(Value::Bool(true))
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        let _ = evaluate(&app, "document.querySelector('.new-task').click();true").await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        let mut layouts = vec![];
        if let Some(window) = app.get_webview_window("main") {
            for (width, height) in [(1180., 820.), (960., 660.)] {
                let _ = window.set_size(tauri::LogicalSize::new(width, height));
                tokio::time::sleep(Duration::from_millis(300)).await;
                let layout = evaluate(&app, r#"JSON.stringify((()=>{const m=document.querySelector('.batch-modal'),f=m?.querySelector('.modal-footer'),t=m?.querySelector('textarea');if(!m||!f||!t)return {valid:false};const r=m.getBoundingClientRect(),b=f.getBoundingClientRect();return {valid:r.left>=0&&r.top>=0&&r.right<=innerWidth&&r.bottom<=innerHeight&&b.bottom<=r.bottom+1&&b.height>0&&m.scrollWidth<=m.clientWidth,viewport:[innerWidth,innerHeight],footerHeight:b.height,bodyScrolls:m.querySelector('.batch-body').scrollHeight>m.querySelector('.batch-body').clientHeight,inputFocused:document.activeElement===t}})())"#).await.unwrap_or_default();
                layouts.push(layout);
            }
            let _ = window.set_size(tauri::LogicalSize::new(1180., 820.));
        }
        // Two different parts plus a duplicate exercise extraction, ordering and real queue starts.
        let links = "https://www.bilibili.com/video/BV17x411w7KC?p=10\nhttps://www.bilibili.com/video/BV17x411w7KC?p=9\nhttps://www.bilibili.com/video/BV17x411w7KC?p=10&tracking=fixture";
        let script = format!("(()=>{{const field=document.querySelector('.batch-modal textarea');Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype,'value').set.call(field,{});field.dispatchEvent(new Event('input',{{bubbles:true}}));return true;}})()", serde_json::to_string(links).unwrap());
        let _ = evaluate(&app, &script).await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        let _ = evaluate(&app, "document.querySelector('.batch-submit').click();true").await;
        let mut seen = HashMap::new();
        let mut completed = false;
        for _ in 0..1200 {
            {
                let tasks = state.tasks.lock().unwrap();
                for task in tasks.iter().filter(|t| t.status != TaskStatus::Queued) {
                    seen.entry(task.id.clone()).or_insert(now_ms());
                }
                if tasks.len() == 2 && tasks.iter().all(|t| t.status == TaskStatus::Completed) {
                    completed = true;
                    break;
                }
                if tasks.iter().any(|t| t.status == TaskStatus::Failed) {
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let tasks = state.tasks.lock().unwrap().clone();
        let mut times = seen.values().copied().collect::<Vec<_>>();
        times.sort();
        let gap = if times.len() == 2 {
            times[1].saturating_sub(times[0])
        } else {
            0
        };
        let mut playable = vec![];
        for task in &tasks {
            let Some(path) = &task.output_path else {
                continue;
            };
            let mut probe = tokio::process::Command::new(
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/ffprobe.exe"),
            );
            probe
                .args([
                    "-v",
                    "error",
                    "-show_entries",
                    "stream=codec_type",
                    "-of",
                    "json",
                ])
                .arg(path);
            #[cfg(windows)]
            probe.creation_flags(0x08000000);
            if let Ok(output) = probe.output().await {
                let value: Value = serde_json::from_slice(&output.stdout).unwrap_or_default();
                let streams = value["streams"].as_array();
                playable.push(streams.is_some_and(|s| {
                    s.iter().any(|v| v["codec_type"] == "video")
                        && s.iter().any(|v| v["codec_type"] == "audio")
                }));
            }
        }
        let closed = evaluate(&app, "!document.querySelector('.batch-modal')").await
            == Some(Value::Bool(true));
        let unchanged = serde_json::to_value(&*state.settings.lock().unwrap()).ok()
            == serde_json::to_value(&settings).ok();
        let success = completed
            && closed
            && tasks.len() == 2
            && playable.len() == 2
            && playable.iter().all(|p| *p)
            && gap >= 2900
            && layouts.iter().all(|l| l["valid"] == true)
            && unchanged;
        let report = json!({"success":success,"completed":completed,"dialogClosed":closed,"taskCount":tasks.len(),"observedStartGapMs":gap,"configuredGapSecs":3,"layouts":layouts,"playableVideoAndAudio":playable,"globalSettingsUnchanged":unchanged,"tasks":tasks.iter().map(|t| json!({"page":t.page,"status":t.status,"error":t.error,"intervalSecs":t.start_interval_secs})).collect::<Vec<_>>()});
        let _ = tokio::fs::write(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../.tools/v015-batch-verification.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .await;
        // Leave the new modal open for a visual check, with no account details or signed links.
        let _ = evaluate(&app, "document.querySelector('.new-task').click();true").await;
    });
}
