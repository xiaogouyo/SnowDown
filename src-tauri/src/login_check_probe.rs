//! Opt-in Windows regression test. All platform responses and cookies below are
//! fixtures in an isolated application/profile; no user account is accessed.
use crate::{batch_probe::evaluate, model::Platform, site_sessions, AppState};
use serde_json::json;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tauri::{AppHandle, Manager};

async fn fixture(
    window: &tauri::WebviewWindow,
    mode: Arc<AtomicUsize>,
    clicks: Arc<AtomicUsize>,
) -> bool {
    use webview2_com::{
        CoTaskMemPWSTR, Microsoft::Web::WebView2::Win32::COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
        WebResourceRequestedEventHandler,
    };
    use windows::{
        core::{w, Interface, PWSTR},
        Win32::System::Com::IStream,
    };
    #[link(name = "shlwapi")]
    extern "system" {
        fn SHCreateMemStream(bytes: *const u8, size: u32) -> *mut std::ffi::c_void;
    }
    let (tx, rx) = tokio::sync::oneshot::channel();
    let result=window.with_webview(move |view|unsafe{
        let installed=(||->windows::core::Result<()>{
            let core=view.controller().CoreWebView2()?;let environment=view.environment();
            core.AddWebResourceRequestedFilter(w!("*"),COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL)?;
            let handler=WebResourceRequestedEventHandler::create(Box::new(move |_,args|{
                let Some(args)=args else{return Ok(());};let mut uri=PWSTR::null();args.Request()?.Uri(&mut uri)?;
                if CoTaskMemPWSTR::from(uri).to_string().contains("snowdown-clicked"){clicks.fetch_add(1,Ordering::SeqCst);}
                let qr=r#"<img class="qrcode" style="width:160px;height:160px" src="data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jT0kAAAAASUVORK5CYII=">"#;
                let body=match mode.load(Ordering::SeqCst){
                    0=>format!(r#"<button onclick="fetch('/snowdown-clicked');document.getElementById('qr').hidden=false">登录</button><div id="qr" hidden>{qr}<p>扫码登录</p></div>"#),
                    1=>format!("<div>{qr}<p>二维码已过期</p></div>"),
                    2=>format!(r#"<a href="/user/profile/1234567890abcdef12345678">我</a><div>{qr}<p>下载手机 App</p></div>"#),
                    _=>"<div class='captcha'>拖动滑块完成安全验证</div>".into(),
                };
                let html=format!("<!doctype html><meta charset=utf-8><body>{body}</body>");let bytes=html.as_bytes();
                let raw=SHCreateMemStream(bytes.as_ptr(),bytes.len() as u32);if raw.is_null(){return Ok(());}
                let stream=IStream::from_raw(raw);
                let response=environment.CreateWebResourceResponse(&stream,200,w!("OK"),w!("Content-Type: text/html; charset=utf-8\r\nCache-Control: no-store"))?;
                args.SetResponse(&response)?;Ok(())
            }));
            let mut token=0;core.add_WebResourceRequested(&handler,&mut token)?;Ok(())
        })();let _=tx.send(installed.is_ok());
    });
    result.is_ok()
        && tokio::time::timeout(Duration::from_secs(5), rx)
            .await
            .ok()
            .and_then(Result::ok)
            == Some(true)
}
async fn ui(app: &AppHandle, script: &str, ticks: usize) -> bool {
    for _ in 0..ticks {
        if evaluate(app, script).await == Some(json!(true)) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    false
}
pub fn start(app: AppHandle, data: PathBuf) {
    tauri::async_runtime::spawn(async move {
        let result=async{
            tokio::time::sleep(Duration::from_secs(5)).await;
            let state=app.state::<Arc<AppState>>();let platform=Platform::Xiaohongshu;
            let window=site_sessions::login_window(&app,platform)?;
            let mode=Arc::new(AtomicUsize::new(0));let clicks=Arc::new(AtomicUsize::new(0));
            if !fixture(&window,mode.clone(),clicks.clone()).await{return Err("Could not install isolated response fixture".to_string());}
            crate::web_capture::clear_profile(&window).await?;
            state.site_sessions.save(platform,"web_session=isolated-login-check-fixture; a1=isolated-device".into())?;
            state.site_sessions.save_profile(platform,site_sessions::Profile{name:"隔离检查测试账号".into(),avatar:String::new()})?;
            let saved=data.join("site-sessions/xiaohongshu.dat");let original=tokio::fs::read(&saved).await.map_err(|_|"Fixture file missing")?;
            let live_before=site_sessions::capture(&app,platform).await?;
            state.emit(&app);
            if !ui(&app,"!!document.querySelector('.profile')",40).await{return Err("Main renderer not ready".into());}
            evaluate(&app,"document.querySelector('.profile').click();true").await;
            evaluate(&app,"document.querySelectorAll('.account-platform-tabs button')[2].click();true").await;
            evaluate(&app,"document.querySelectorAll('.account-buttons button')[1].click();true").await;
            let unknown_ui=ui(&app,"document.querySelector('.toast-message')?.textContent.includes('已保留')===true",130).await;
            let unknown_saved=tokio::fs::read(&saved).await.ok().as_ref()==Some(&original);
            let restored=site_sessions::capture(&app,platform).await?.contains("web_session=isolated-login-check-fixture");
            let _=crate::minimal_probe::capture_label(&app,"main","login-check-retained").await;
            mode.store(1,Ordering::SeqCst);
            let expired=state.qr_login.check(&app,platform).await?;
            let expired_saved=tokio::fs::read(&saved).await.ok().as_ref()==Some(&original);
            mode.store(2,Ordering::SeqCst);
            let check_started=std::time::Instant::now();
            let authenticated=state.qr_login.check(&app,platform).await?;
            let authenticated_ms=check_started.elapsed().as_millis();
            let cached_started=std::time::Instant::now();
            let recent=state.qr_login.check(&app,platform).await?;
            let recent_ms=cached_started.elapsed().as_millis();
            mode.store(3,Ordering::SeqCst);
            state.qr_login.cancel(&app,platform);
            let before_verification=tokio::fs::read(&saved).await.map_err(|_|"Saved login missing")?;
            let verification=state.qr_login.check(&app,platform).await?;
            let verification_saved=tokio::fs::read(&saved).await.ok().as_ref()==Some(&before_verification);
            mode.store(0,Ordering::SeqCst);
            let (check,logout)=tokio::join!(state.qr_login.check(&app,platform),async{
                tokio::time::sleep(Duration::from_millis(400)).await;
                state.qr_login.cancel(&app,platform);state.site_sessions.logout(&app,platform).await
            });
            tokio::time::sleep(Duration::from_millis(200)).await;
            let logout_cleared=logout.is_ok()&&check.is_err()&&!saved.exists()&&!site_sessions::capture(&app,platform).await?.contains("web_session=");
            let mut imported=true;
            for platform in [Platform::Xiaohongshu,Platform::Douyin] {
                let window=site_sessions::login_window(&app,platform)?;
                let auth=if platform==Platform::Douyin{"sessionid"}else{"web_session"};
                let header=format!("{auth}=isolated-import-fixture; a1=isolated-script-token");
                state.site_sessions.import_browser(&app,platform,header).await?;
                let live=site_sessions::capture(&app,platform).await?;
                let cookies=tauri::async_runtime::spawn_blocking(move ||window.cookies_for_url(platform.home().parse().unwrap())).await.map_err(|_|"Cookie metadata unavailable")?.map_err(|_|"Cookie metadata unavailable")?;
                imported &= live.contains(&format!("{auth}=isolated-import-fixture")) && cookies.iter().any(|c|c.name()==auth&&c.http_only()==Some(true)) && cookies.iter().any(|c|c.name()=="a1"&&c.http_only()==Some(false));
                state.site_sessions.logout(&app,platform).await?;
            }
            let success=unknown_ui&&unknown_saved&&restored&&expired.state=="unknown"&&expired_saved&&authenticated.state=="authenticated"&&verification.state=="unknown"&&verification_saved&&logout_cleared&&imported&&recent.state=="authenticated"&&recent_ms<200&&authenticated_ms<2500&&clicks.load(Ordering::SeqCst)==0;
            Ok::<_,String>(json!({"success":success,"initialWebviewHadNoSession":!live_before.contains("web_session="),"unknownUi":unknown_ui,"unknownFileUnchanged":unknown_saved,"savedCookieRestored":restored,"expiredQrState":expired.state,"expiredQrFileUnchanged":expired_saved,"positiveAccountState":authenticated.state,"verificationState":verification.state,"verificationFileUnchanged":verification_saved,"loginButtonClicks":clicks.load(Ordering::SeqCst),"logoutCannotBeUndone":logout_cleared,"bothPlatformImportsAndCookieFlags":imported,"usesOnlyFixtures":true,"authenticatedMs":authenticated_ms,"recentCheckMs":recent_ms}))
        }.await;
        let report = match result {
            Ok(value) => value,
            Err(error) => json!({"success":false,"error":error}),
        };
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../.tools/v021-login-verification.json");
        let _ = tokio::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).await;
        app.exit(if report["success"] == true { 0 } else { 1 });
    });
}
