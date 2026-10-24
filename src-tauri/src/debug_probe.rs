// Opt-in desktop integration checks. This module is excluded from release builds.
use crate::{engine, sites, AppState};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::{AppHandle, Manager};

async fn ui_state(app: &AppHandle, input: &str, id: &str) -> Option<serde_json::Value> {
    let window = app.get_webview_window("main")?;
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let sender = Mutex::new(Some(sender));
    let script = format!("JSON.stringify({{modalVisible:!!document.querySelector('.video-modal'),sourceMatches:document.querySelector('.video-modal small')?.textContent.includes({})??false,titleVisible:!!document.querySelector('.video-summary h3')?.textContent.trim(),qualityVisible:!!document.querySelector('.video-modal .ui-select'),downloadVisible:!!document.querySelector('.video-modal .modal-footer button'),inputMatches:document.querySelector('input[aria-label=\"视频链接\"]')?.value==={}}})", serde_json::to_string(id).ok()?, serde_json::to_string(input).ok()?);
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
    let value: serde_json::Value = serde_json::from_str(&raw).ok()?;
    if let Some(text) = value.as_str() {
        serde_json::from_str(text).ok()
    } else {
        Some(value)
    }
}

async fn readable(client: &reqwest::Client, url: &str) -> bool {
    let Ok(mut response) = client
        .get(url)
        .header(reqwest::header::RANGE, "bytes=0-4095")
        .send()
        .await
    else {
        return false;
    };
    response.status().is_success()
        && response
            .chunk()
            .await
            .ok()
            .flatten()
            .is_some_and(|chunk| !chunk.is_empty())
}

pub fn start(app: AppHandle) {
    let Ok(input) = std::env::var("SNOWDOWN_TEST_SOURCE") else {
        return;
    };
    let Ok(source) = sites::recognize(&input) else {
        return;
    };
    let Ok(id) = sites::video_id(&source) else {
        return;
    };
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let Some(window) = app.get_webview_window("main") else {
            return;
        };
        let script = format!("(()=>{{const field=document.querySelector('input[aria-label=\"视频链接\"]');Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(field,{});field.dispatchEvent(new Event('input',{{bubbles:true}}));document.querySelector('.link-form').requestSubmit();}})()", serde_json::to_string(&source.url).unwrap());
        let _ = window.eval(script);
        let mut result = serde_json::json!({"success":false});
        for _ in 0..45 {
            tokio::time::sleep(Duration::from_secs(1)).await;
            if let Some(value) = ui_state(&app, &source.url, &id).await {
                let ready = [
                    "modalVisible",
                    "sourceMatches",
                    "titleVisible",
                    "qualityVisible",
                    "downloadVisible",
                    "inputMatches",
                ]
                .iter()
                .all(|key| value[*key] == true);
                result = value;
                if ready {
                    let state = app.state::<Arc<AppState>>();
                    if let Ok(resolved) = state.site_sessions.resolve(&app, &source, 0).await {
                        if let Ok(client) = engine::media_client(source.platform) {
                            result["videoReadable"] = readable(&client, &resolved.url).await.into();
                            result["audioReadable"] = match &resolved.audio_url {
                                Some(url) => readable(&client, url).await,
                                None => true,
                            }
                            .into();
                        }
                    }
                    result["loginWindowExists"] = app
                        .get_webview_window(&format!("official-{}", source.platform.key()))
                        .is_some()
                        .into();
                    result["parserVisible"] = app
                        .get_webview_window(&format!("resolver-{}", source.platform.key()))
                        .and_then(|w| w.is_visible().ok())
                        .into();
                    result["success"] = (result["videoReadable"] == true
                        && result["audioReadable"] == true
                        && result["loginWindowExists"] == false
                        && result["parserVisible"] != true)
                        .into();
                    break;
                }
            }
        }
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../.tools/v013-parse-verification.json");
        let _ = tokio::fs::write(path, serde_json::to_vec(&result).unwrap()).await;
    });
}
