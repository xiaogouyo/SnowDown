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
    if std::env::var_os("SNOWDOWN_QR_VERIFY").is_some() {
        verify_qr(app);
        return;
    }
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

fn verify_qr(app: AppHandle) {
    use crate::model::Platform;
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
    tauri::async_runtime::spawn(async move {
        let qa_login = crate::qr_login::QrLogin::default();
        let auth = crate::auth::Auth::open(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.tools/qr-qa-bilibili.dat"),
        );
        let result = auth.begin().await;
        let mut report = serde_json::json!({"platform":"bilibili","success":result.is_ok(),"error":result.as_ref().err()});
        if let Ok(qr) = result {
            report["imagePresent"] = (!qr.image.is_empty()).into();
            report["poll"] = auth
                .poll(&qr.id)
                .await
                .map(|poll| poll.state.to_string())
                .unwrap_or_else(|error| error)
                .into();
            auth.cancel();
            if let Ok(fresh) = auth.begin().await {
                report["refreshChangedImage"] = (fresh.image != qr.image).into();
            }
            auth.cancel();
        }
        let _ = tokio::fs::write(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.tools/qr-verify-bilibili.json"),
            serde_json::to_vec(&report).unwrap(),
        )
        .await;
        for platform in [Platform::Douyin, Platform::Xiaohongshu] {
            if let Ok(window) = crate::site_sessions::login_window(&app, platform) {
                let _ = crate::web_capture::clear_profile(&window).await;
            }
            let result = qa_login.begin(&app, platform).await;
            let mut report = serde_json::json!({"platform":platform,"success":result.is_ok(),"error":result.as_ref().err()});
            if let Ok(qr) = result {
                report["imagePresent"] = (!qr.image.is_empty()).into();
                report["expiresInMs"] = qr.expires_at.saturating_sub(crate::model::now_ms()).into();
                report["windowVisible"] = app
                    .get_webview_window(&format!("official-{}", platform.key()))
                    .and_then(|w| w.is_visible().ok())
                    .into();
                // Save only a local preview for visual inspection, never publish a live login QR.
                use base64::Engine;
                if let Some((_, data)) = qr.image.split_once(',') {
                    if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(data) {
                        let _ = tokio::fs::write(
                            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                                .join(format!("../.tools/qr-preview-{}.png", platform.key())),
                            bytes,
                        )
                        .await;
                    }
                }
                report["poll"] = qa_login
                    .poll(&app, platform, &qr.id)
                    .await
                    .map(|p| p.state)
                    .map_err(|e| e)
                    .unwrap_or_else(|e| e)
                    .into();
                qa_login.cancel(&app, platform);
                if let Ok(fresh) = qa_login.begin(&app, platform).await {
                    report["refreshChangedImage"] = (fresh.image != qr.image).into();
                }
                if let Some(window) =
                    app.get_webview_window(&format!("official-{}", platform.key()))
                {
                    // The flag selects isolated QA profiles; never seed or clear a user's profile.
                    let domain = if platform == Platform::Douyin {
                        ".douyin.com"
                    } else {
                        ".xiaohongshu.com"
                    };
                    let cookie = tauri::webview::Cookie::build(("snowdown_qa", "fixture"))
                        .domain(domain)
                        .path("/")
                        .secure(true)
                        .build();
                    let _ = window.set_cookie(cookie);
                    async fn cookie_state(window: tauri::WebviewWindow) -> serde_json::Value {
                        tauri::async_runtime::spawn_blocking(move || {
                            match window.cookies() {
                                Ok(cookies)=>serde_json::json!({"count":cookies.len(),"fixture":cookies.iter().any(|cookie|cookie.name()=="snowdown_qa")}),
                                Err(_)=>serde_json::json!({"error":"cookie-read-failed"})
                            }
                        })
                        .await
                        .unwrap_or_default()
                    }
                    report["profileBefore"] = cookie_state(window.clone()).await;
                    let _ = window.navigate("about:blank".parse().unwrap());
                    let clear = crate::web_capture::clear_profile(&window).await;
                    report["profileClearCompleted"] = clear.is_ok().into();
                    report["profileAfter"] = cookie_state(window).await;
                }
                qa_login.cancel(&app, platform);
            }
            if report["imagePresent"]!=true {
                if let Some(window)=app.get_webview_window(&format!("official-{}",platform.key())) {
                    let (sender,receiver)=tokio::sync::oneshot::channel();let sender=Mutex::new(Some(sender));
                    let _=window.eval_with_callback(r#"JSON.stringify({ready:document.readyState,path:location.pathname,requested:window.__snowdownQrRequested,loginButtons:[...document.querySelectorAll('button')].filter(e=>e.textContent.trim()==='登录').map(e=>({cls:e.className,visible:e.getBoundingClientRect().width>0,form:!!e.closest('form,.login-container,.login-modal')})),images:[...document.querySelectorAll('img')].filter(e=>e.src.startsWith('data:')).slice(0,12).map(e=>({cls:e.className,width:e.getBoundingClientRect().width,height:e.getBoundingClientRect().height}))})"#,move|value|{if let Some(sender)=sender.lock().unwrap().take(){let _=sender.send(value);}});
                    if let Ok(Ok(raw))=tokio::time::timeout(Duration::from_secs(3),receiver).await {
                        let value:serde_json::Value=serde_json::from_str(&raw).unwrap_or_default();
                        report["dom"]=if let Some(text)=value.as_str(){serde_json::from_str(text).unwrap_or_default()}else{value};
                    }
                }
            }
            let _ = tokio::fs::write(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join(format!("../.tools/qr-verify-{}.json", platform.key())),
                serde_json::to_vec(&report).unwrap(),
            )
            .await;
        }
    });
}
