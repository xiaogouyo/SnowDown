//! Opt-in native QA. Only public samples, isolated data and an isolated WebView profile.
use crate::{batch_probe::evaluate, model::TaskStatus, updater::Phase, AppState};
use serde_json::{json, Value};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tauri::{AppHandle, Emitter, Manager};

async fn wait(app: &AppHandle, script: &str, ticks: usize) -> bool {
    for _ in 0..ticks {
        if evaluate(app, script).await == Some(json!(true)) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    false
}

async fn progress(value: Value) {
    let _ = tokio::fs::write(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.tools/v019-qa-progress.json"),
        serde_json::to_vec_pretty(&value).unwrap(),
    )
    .await;
}

// Capture our own WebView's rendered surface as a development test artifact.
// This uses the WebView2 renderer, not screen scraping or another application's UI.
#[cfg(windows)]
async fn capture(app: &AppHandle, name: &str) -> bool {
    use webview2_com::{
        CapturePreviewCompletedHandler,
        Microsoft::Web::WebView2::Win32::COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG,
    };
    use windows::{
        core::Interface,
        Win32::System::Com::{IStream, STATFLAG_NONAME, STATSTG, STREAM_SEEK_SET},
    };
    #[link(name = "ole32")]
    extern "system" {
        fn CreateStreamOnHGlobal(
            memory: *mut std::ffi::c_void,
            delete: i32,
            stream: *mut *mut std::ffi::c_void,
        ) -> i32;
    }
    let Some(window) = app.get_webview_window("main") else {
        return false;
    };
    let (sender, receiver) = tokio::sync::oneshot::channel();
    if window
        .with_webview(move |view| unsafe {
            let mut raw = std::ptr::null_mut();
            if CreateStreamOnHGlobal(std::ptr::null_mut(), 1, &mut raw) < 0 || raw.is_null() {
                return;
            }
            let stream = IStream::from_raw(raw);
            let output = stream.clone();
            let callback = CapturePreviewCompletedHandler::create(Box::new(move |result| {
                result?;
                let mut stat = STATSTG::default();
                output.Stat(&mut stat, STATFLAG_NONAME)?;
                if stat.cbSize > 20 * 1024 * 1024 {
                    return Ok(());
                }
                output.Seek(0, STREAM_SEEK_SET, None)?;
                let mut bytes = vec![0u8; stat.cbSize as usize];
                let mut received = 0;
                output
                    .Read(
                        bytes.as_mut_ptr().cast(),
                        bytes.len() as u32,
                        Some(&mut received),
                    )
                    .ok()?;
                bytes.truncate(received as usize);
                let _ = sender.send(bytes);
                Ok(())
            }));
            if let Ok(core) = view.controller().CoreWebView2() {
                let _ = core.CapturePreview(
                    COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG,
                    &stream,
                    &callback,
                );
            }
        })
        .is_err()
    {
        return false;
    }
    let Ok(Ok(bytes)) = tokio::time::timeout(Duration::from_secs(10), receiver).await else {
        return false;
    };
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.tools/v019-screenshots");
    let _ = tokio::fs::create_dir_all(&directory).await;
    tokio::fs::write(directory.join(format!("{name}.png")), bytes)
        .await
        .is_ok()
}

#[cfg(not(windows))]
async fn capture(_: &AppHandle, _: &str) -> bool {
    false
}

pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(6)).await;
        let state = app.state::<Arc<AppState>>();
        let window = app.get_webview_window("main").unwrap();
        let _ = evaluate(
            &app,
            "document.querySelector('[aria-label=\"关闭版本更新\"]')?.click();true",
        )
        .await;
        let _ = wait(&app, "!document.querySelector('.update-modal')", 60).await;
        let _ = evaluate(
            &app,
            "if(!document.querySelector('.minimal-mode'))document.querySelector('[aria-label=\"切换到简约模式\"]').click();true",
        )
        .await;
        tokio::time::sleep(Duration::from_millis(650)).await;
        let _ = evaluate(
            &app,
            "document.querySelector('[aria-label=\"关闭版本更新\"]')?.click();true",
        )
        .await;
        let _ = wait(&app, "!document.querySelector('.update-modal')", 60).await;
        let center=evaluate(&app,r#"JSON.stringify((()=>{const f=document.querySelector('.minimal-input-bar'),r=f?.getBoundingClientRect();return {centered:!!r&&Math.abs(r.top+r.height/2-innerHeight/2)<2,width:r?.width,height:r?.height,top:r?.top,noShell:getComputedStyle(document.querySelector('.standard-mode')).display==='none',noModal:!document.querySelector('.modal-overlay')}})())"#).await.unwrap_or_default();
        let empty_image = capture(&app, "empty").await;
        let script = r#"(()=>{const f=document.querySelector('[aria-label="简约视频链接"]');Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(f,'https://www.bilibili.com/video/BV17x411w7KC?p=10');f.dispatchEvent(new Event('input',{bubbles:true}));document.querySelector('.minimal-input-bar').requestSubmit();return true})()"#;
        let _ = evaluate(&app, script).await;
        tokio::time::sleep(Duration::from_millis(170)).await;
        let middle = evaluate(
            &app,
            "document.querySelector('.minimal-input-bar').getBoundingClientRect().top",
        )
        .await
        .unwrap_or_default();
        let parsed = wait(
            &app,
            "!!document.querySelector('.minimal-result h1')?.textContent.trim()",
            220,
        )
        .await;
        if !parsed {
            let failure = json!({"success":false,"stage":"parse did not complete","center":center,"ui":evaluate(&app,"document.body.innerText").await});
            let _ = tokio::fs::write(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../.tools/v019-minimal-verification.json"),
                serde_json::to_vec_pretty(&failure).unwrap(),
            )
            .await;
            app.exit(1);
            return;
        }
        tokio::time::sleep(Duration::from_millis(650)).await;
        let top=evaluate(&app,r#"JSON.stringify((()=>{const f=document.querySelector('.minimal-input-bar').getBoundingClientRect(),b=document.querySelector('.minimal-download-row .primary')?.getBoundingClientRect();return {top:f.top,width:f.width,height:f.height,inline:!!document.querySelector('.minimal-result'),noModal:!document.querySelector('.modal-overlay'),downloadVisible:!!b&&b.bottom<=innerHeight&&b.top>=0,rootFits:document.documentElement.scrollWidth<=innerWidth}})())"#).await.unwrap_or_default();
        let result_image = capture(&app, "parsed").await;
        let _ = window.set_size(tauri::LogicalSize::new(960., 660.));
        tokio::time::sleep(Duration::from_millis(600)).await;
        let minimum=evaluate(&app,r#"JSON.stringify((()=>{const f=document.querySelector('.minimal-input-bar').getBoundingClientRect(),b=document.querySelector('.minimal-download-row .primary')?.getBoundingClientRect(),c=document.querySelector('.minimal-body');return {inputFits:f.left>=0&&f.right<=innerWidth,downloadVisible:!!b&&b.bottom<=innerHeight&&b.top>=0,rootFits:document.documentElement.scrollWidth<=innerWidth,bodyScrollable:c?.scrollHeight>c?.clientHeight,viewport:[innerWidth,innerHeight]}})())"#).await.unwrap_or_default();
        let minimum_image = capture(&app, "minimum").await;
        let _ = window.set_size(tauri::LogicalSize::new(1180., 820.));
        tokio::time::sleep(Duration::from_millis(400)).await;
        progress(json!({"stage":"inline media download","parsed":parsed,"minimum":minimum})).await;
        let _ = evaluate(
            &app,
            "document.querySelector('.minimal-download-row .primary')?.click();true",
        )
        .await;
        let mut media_complete = false;
        let mut media_error = None;
        for _ in 0..1200 {
            {
                let tasks = state.tasks.lock().unwrap();
                if tasks
                    .iter()
                    .any(|task| task.status == TaskStatus::Completed)
                {
                    media_complete = true;
                    break;
                }
                if let Some(task) = tasks.iter().find(|task| task.status == TaskStatus::Failed) {
                    media_error = task.error.clone();
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
        let inline_progress = wait(
            &app,
            "!!document.querySelector('.minimal-task [aria-valuenow=\"100\"]')",
            20,
        )
        .await;
        let media_image = capture(&app, "downloaded").await;
        let _=evaluate(&app,"document.querySelector('[aria-label=\"清空并重新输入\"]')?.click();document.querySelector('[aria-label=\"切换回完整模式\"]')?.click();true").await;
        tokio::time::sleep(Duration::from_millis(600)).await;
        let _ = evaluate(
            &app,
            "document.querySelector('.version-button').click();true",
        )
        .await;
        let update_available = wait(
            &app,
            "!!document.querySelector('.update-download:not(:disabled)')",
            180,
        )
        .await;
        let mut started = false;
        for _ in 0..60 {
            if matches!(
                state.updater.status().phase,
                Phase::Probing | Phase::Downloading | Phase::Ready
            ) {
                started = true;
                break;
            }
            let _=evaluate(&app,"(()=>{const b=document.querySelector('.update-download:not(:disabled)');if(!b)return false;b.click();return true})()").await;
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
        let start_ui=evaluate(&app,"JSON.stringify({error:document.querySelector('.update-error')?.textContent,heading:document.querySelector('#update-title')?.textContent,background:!!document.querySelector('.update-background')})").await.unwrap_or_default();
        let _ = capture(&app, "update-start").await;
        let _ = evaluate(
            &app,
            "document.querySelector('.update-background')?.click();true",
        )
        .await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        let panel_closed =
            evaluate(&app, "!document.querySelector('.update-modal')").await == Some(json!(true));
        let _ = evaluate(
            &app,
            "document.querySelector('[aria-label=\"切换到简约模式\"]').click();true",
        )
        .await;
        tokio::time::sleep(Duration::from_millis(450)).await;
        let _ = window.hide();
        let mut hidden_active = false;
        let mut update_ready = false;
        let mut update_error = None;
        for tick in 0..1800 {
            let status = state.updater.status();
            if window.is_visible().ok() == Some(false)
                && matches!(status.phase, Phase::Probing | Phase::Downloading)
            {
                hidden_active = true
            }
            if tick % 10 == 0 {
                progress(json!({"stage":"hidden native update download","phase":status.phase,"downloaded":status.downloaded,"total":status.total,"source":status.download_source,"tested":status.mirrors.iter().filter(|mirror|!matches!(mirror.state,crate::update_mirrors::MirrorState::Pending|crate::update_mirrors::MirrorState::Testing)).count()})).await;
            }
            if status.phase == Phase::Ready {
                update_ready = true;
                break;
            }
            if status.phase == Phase::Error {
                update_error = status.error;
                break;
            }
            if tick > 10 && status.phase == Phase::Idle {
                update_error = Some("No native download was started".into());
                break;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        let verified = update_ready && state.updater.verified_installer().await.is_ok();
        let _ = window.show();
        let _ = window.set_focus();
        tokio::time::sleep(Duration::from_millis(400)).await;
        let _ = evaluate(
            &app,
            "document.querySelector('[aria-label=\"切换回完整模式\"]')?.click();true",
        )
        .await;
        tokio::time::sleep(Duration::from_millis(500)).await;
        let _ = evaluate(
            &app,
            "document.querySelector('.version-button').click();true",
        )
        .await;
        let update_restored=wait(&app,"!!document.querySelector('.update-install')&&document.querySelector('.update-modal').textContent.includes('校验通过')",60).await;
        let update_image = capture(&app, "background-update-ready").await;
        let animation = center["top"]
            .as_f64()
            .zip(middle.as_f64())
            .zip(top["top"].as_f64())
            .is_some_and(|((start, middle), end)| start > middle && middle > end);
        let success = parsed
            && center["centered"] == true
            && center["noShell"] == true
            && center["noModal"] == true
            && top["noModal"] == true
            && top["rootFits"] == true
            && minimum["rootFits"] == true
            && minimum["downloadVisible"] == true
            && animation
            && media_complete
            && inline_progress
            && update_available
            && started
            && panel_closed
            && hidden_active
            && verified
            && update_restored;
        let report = json!({"success":success,"center":center,"animationMiddleTop":middle,"top":top,"minimum":minimum,"animation":animation,"parsed":parsed,"mediaComplete":media_complete,"mediaError":media_error,"inlineProgress":inline_progress,"updateAvailable":update_available,"updateStarted":started,"startUi":start_ui,"panelClosed":panel_closed,"hiddenUpdateContinued":hidden_active,"updateReady":update_ready,"installerHashVerified":verified,"installerExecuted":false,"updateRestored":update_restored,"updateError":update_error,"screenshots":{"empty":empty_image,"parsed":result_image,"minimum":minimum_image,"downloaded":media_image,"updateReady":update_image},"systemClipboardModified":false});
        let _ = tokio::fs::write(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../.tools/v019-minimal-verification.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .await;
        let _ = app.emit("snowdown://update", state.updater.status());
        app.exit(0);
    });
}
