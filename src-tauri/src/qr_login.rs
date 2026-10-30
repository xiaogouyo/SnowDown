use crate::{
    auth,
    model::{now_ms, Platform},
    site_sessions, AppState,
};
use serde::{Deserialize, Serialize};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tauri::{AppHandle, Manager};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginQr {
    pub id: String,
    pub image: String,
    pub expires_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
}
impl From<auth::LoginQr> for LoginQr {
    fn from(qr: auth::LoginQr) -> Self {
        Self {
            id: qr.id,
            image: qr.image,
            expires_at: qr.expires_at,
            state: None,
        }
    }
}
#[derive(Serialize)]
pub struct LoginPoll {
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qr: Option<LoginQr>,
}
impl LoginPoll {
    pub fn state(state: &str) -> Self {
        Self {
            state: state.into(),
            qr: None,
        }
    }
}

#[derive(Serialize)]
pub struct LoginCheck {
    pub state: &'static str,
    pub message: String,
}
impl LoginCheck {
    pub fn new(state: &'static str, message: impl Into<String>) -> Self {
        Self {
            state,
            message: message.into(),
        }
    }
}
#[derive(Default)]
struct CheckEvidence {
    confirmed: usize,
}
impl CheckEvidence {
    fn observe(&mut self, state: &str, elapsed: Duration) -> bool {
        self.confirmed = if state == "confirmed" {
            self.confirmed + 1
        } else {
            0
        };
        self.confirmed >= 2 && elapsed >= Duration::from_millis(700)
    }
}

#[derive(Default, Deserialize)]
struct Frame {
    state: String,
    #[serde(default)]
    image: String,
    #[serde(default)]
    account: Option<site_sessions::Profile>,
}
#[derive(Clone)]
struct Pending {
    qr: LoginQr,
    platform: Platform,
    generation: u64,
}
#[derive(Default)]
struct Inner {
    generation: u64,
    pending: Option<Pending>,
    platform: Option<Platform>,
}
#[derive(Default)]
pub struct QrLogin {
    inner: Mutex<Inner>,
    checked: Mutex<std::collections::HashMap<Platform, (String, Instant)>>,
}

fn valid_image(image: &str) -> bool {
    use base64::Engine;
    let Some((kind, data)) = image.split_once(',') else {
        return false;
    };
    if !matches!(
        kind,
        "data:image/png;base64" | "data:image/jpeg;base64" | "data:image/webp;base64"
    ) || data.len() > 1024 * 1024
    {
        return false;
    }
    base64::engine::general_purpose::STANDARD
        .decode(data)
        .is_ok_and(|bytes| {
            bytes.len() > 64
                && (bytes.starts_with(b"\x89PNG\r\n\x1a\n")
                    || bytes.starts_with(b"\xff\xd8\xff")
                    || (bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP")))
        })
}

fn has_account_cookie(platform: Platform, cookie: &str) -> bool {
    cookie
        .split(';')
        .filter_map(|item| item.trim().split_once('='))
        .any(|(name, value)| {
            !value.is_empty()
                && match platform {
                    Platform::Douyin => matches!(name, "sessionid" | "sessionid_ss"),
                    Platform::Xiaohongshu => matches!(name, "web_session" | "web_session_ssl"),
                    Platform::Bilibili | Platform::Web | Platform::Channels => false,
                }
        })
}

async fn frame(app: &AppHandle, platform: Platform, check_only: bool) -> Result<Frame, String> {
    let window = app
        .get_webview_window(&format!("official-{}", platform.key()))
        .ok_or("官方登录页面已关闭，请重新获取二维码。")?;
    let url = window.url().map_err(|_| "官方登录页面不可用。")?;
    if url.scheme() != "https"
        || !crate::sites::host_matches(
            url.host_str().unwrap_or_default(),
            match platform {
                Platform::Douyin => "douyin.com",
                _ => "xiaohongshu.com",
            },
        )
    {
        return Ok(Frame::default());
    }
    let script = include_str!("site_qr.js")
        .replace("__PLATFORM__", &serde_json::to_string(&platform).unwrap())
        .replace("__CHECK_ONLY__", if check_only { "true" } else { "false" });
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let sender = Mutex::new(Some(sender));
    window
        .eval_with_callback(script, move |value| {
            if let Some(sender) = sender.lock().unwrap().take() {
                let _ = sender.send(value);
            }
        })
        .map_err(|_| "官方二维码读取失败。")?;
    let raw = tokio::time::timeout(
        if check_only {
            Duration::from_millis(800)
        } else {
            Duration::from_secs(3)
        },
        receiver,
    )
    .await
    .map_err(|_| "读取二维码超时。")?
    .map_err(|_| "读取二维码失败。")?;
    if raw.len() > 2 * 1024 * 1024 {
        return Err("二维码数据过大。".into());
    }
    let value: serde_json::Value =
        serde_json::from_str(&raw).map_err(|_| "官方二维码数据无效。")?;
    if let Some(text) = value.as_str() {
        serde_json::from_str(text)
    } else {
        serde_json::from_value(value)
    }
    .map_err(|_| "官方二维码数据无效。".into())
}

impl QrLogin {
    fn current(&self, generation: u64) -> bool {
        self.inner.lock().unwrap().generation == generation
    }
    pub fn cancel(&self, app: &AppHandle, platform: Platform) {
        self.checked.lock().unwrap().remove(&platform);
        let mut inner = self.inner.lock().unwrap();
        if inner.platform.is_some_and(|active| active != platform) {
            return;
        }
        inner.generation += 1;
        inner.pending = None;
        inner.platform = None;
        drop(inner);
        if let Some(window) = app.get_webview_window(&format!("official-{}", platform.key())) {
            if !window.is_visible().unwrap_or(false) {
                let _ = window.navigate("about:blank".parse().unwrap());
            }
        }
    }
    async fn complete(
        &self,
        app: &AppHandle,
        platform: Platform,
        generation: u64,
    ) -> Result<(), String> {
        let cookie = site_sessions::capture(app, platform).await?;
        if !has_account_cookie(platform, &cookie) {
            return Err("平台尚未确认账号登录，请在手机上确认后重新获取二维码。".into());
        }
        let account = frame(app, platform, true)
            .await
            .ok()
            .and_then(|frame| frame.account);
        let mut inner = self.inner.lock().unwrap();
        if inner.generation != generation {
            return Err("登录请求已取消。".into());
        }
        app.state::<Arc<AppState>>()
            .site_sessions
            .save(platform, cookie)?;
        if let Some(account) = account {
            let _ = app
                .state::<Arc<AppState>>()
                .site_sessions
                .save_profile(platform, account);
        }
        inner.pending = None;
        inner.platform = None;
        inner.generation += 1;
        drop(inner);
        if let Some(window) = app.get_webview_window(&format!("official-{}", platform.key())) {
            let _ = window.navigate("about:blank".parse().unwrap());
        }
        Ok(())
    }
    pub async fn begin(&self, app: &AppHandle, platform: Platform) -> Result<LoginQr, String> {
        self.checked.lock().unwrap().remove(&platform);
        let generation = {
            let mut inner = self.inner.lock().unwrap();
            inner.generation += 1;
            inner.pending = None;
            inner.platform = Some(platform);
            inner.generation
        };
        let window = site_sessions::login_window(app, platform)?;
        window
            .navigate(platform.home().parse().unwrap())
            .map_err(|_| "无法加载官方登录页面。")?;
        let id = uuid::Uuid::new_v4().to_string();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(25);
        let started = tokio::time::Instant::now();
        let mut confirmed_frames = 0;
        for _ in 0..35 {
            if tokio::time::Instant::now() >= deadline {
                break;
            }
            tokio::time::sleep(Duration::from_millis(600)).await;
            if !self.current(generation) {
                return Err("登录请求已取消。".into());
            }
            let value = match frame(app, platform, false).await {
                Ok(value) => value,
                Err(_) => continue,
            };
            #[cfg(debug_assertions)]
            if std::env::var_os("SNOWDOWN_QR_VERIFY").is_some() {
                let _=tokio::fs::write(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../.tools/qr-frame-{}.json",platform.key())),serde_json::to_vec(&serde_json::json!({"state":value.state,"imageBytes":value.image.len(),"validImage":valid_image(&value.image)})).unwrap()).await;
            }
            if value.state == "verification" {
                return Err("平台暂时要求安全验证，无法获取二维码。请稍后重新获取。".into());
            }
            if value.state == "confirmed" {
                confirmed_frames += 1;
                if started.elapsed() < Duration::from_secs(3) || confirmed_frames < 2 {
                    continue;
                }
                self.complete(app, platform, generation).await?;
                return Ok(LoginQr {
                    id,
                    image: String::new(),
                    expires_at: now_ms(),
                    state: Some("confirmed".into()),
                });
            }
            confirmed_frames = 0;
            if valid_image(&value.image) {
                let qr = LoginQr {
                    id,
                    image: value.image,
                    expires_at: now_ms() + 120_000,
                    state: None,
                };
                let mut inner = self.inner.lock().unwrap();
                if inner.generation != generation {
                    return Err("登录请求已取消。".into());
                }
                inner.pending = Some(Pending {
                    qr: qr.clone(),
                    platform,
                    generation,
                });
                return Ok(qr);
            }
        }
        Err(format!(
            "暂未捕获到{}官方二维码，请检查网络后重试。",
            platform.label()
        ))
    }
    pub async fn poll(
        &self,
        app: &AppHandle,
        platform: Platform,
        id: &str,
    ) -> Result<LoginPoll, String> {
        let pending = self
            .inner
            .lock()
            .unwrap()
            .pending
            .as_ref()
            .filter(|pending| pending.platform == platform && pending.qr.id == id)
            .cloned()
            .ok_or("登录请求已取消，请重新获取二维码。")?;
        let value = frame(app, platform, false).await?;
        if !self.current(pending.generation) {
            return Err("登录请求已取消。".into());
        }
        if value.state == "confirmed" {
            self.complete(app, platform, pending.generation).await?;
            return Ok(LoginPoll::state("confirmed"));
        }
        if value.state == "verification" {
            return Err("平台暂时要求安全验证，请稍后重新获取二维码。".into());
        }
        if valid_image(&value.image) && value.image != pending.qr.image {
            let qr = LoginQr {
                image: value.image,
                expires_at: now_ms() + 120_000,
                ..pending.qr
            };
            let mut inner = self.inner.lock().unwrap();
            if inner.generation != pending.generation {
                return Err("登录请求已取消。".into());
            }
            inner.pending = Some(Pending {
                qr: qr.clone(),
                ..pending
            });
            return Ok(LoginPoll {
                state: "waiting".into(),
                qr: Some(qr),
            });
        }
        if value.state == "expired" || now_ms() >= pending.qr.expires_at {
            return Ok(LoginPoll::state("expired"));
        }
        Ok(LoginPoll::state(if value.state == "scanned" {
            "scanned"
        } else {
            "waiting"
        }))
    }

    pub async fn check(&self, app: &AppHandle, platform: Platform) -> Result<LoginCheck, String> {
        let state = app.state::<Arc<AppState>>();
        let saved = state.site_sessions.cookie(platform);
        if !saved.is_empty()
            && self
                .checked
                .lock()
                .unwrap()
                .get(&platform)
                .is_some_and(|(cookie, time)| {
                    cookie == &saved && time.elapsed() < Duration::from_secs(20)
                })
        {
            return Ok(LoginCheck::new("authenticated", "最近已确认登录有效。"));
        }
        self.cancel(app, platform);
        let generation = {
            let mut inner = self.inner.lock().unwrap();
            inner.platform = Some(platform);
            inner.generation += 1;
            inner.pending = None;
            inner.generation
        };
        // Serialize checks with profile clearing so a cancelled check cannot put
        // cookies back after an explicit logout.
        let _profile = match tokio::time::timeout(
            Duration::from_millis(350),
            state.site_sessions.checking_guard(platform),
        )
        .await
        {
            Ok(value) => value?,
            Err(_) => {
                return Ok(LoginCheck::new(
                    "unknown",
                    "平台正在解析视频或处理账号，请稍后检查；原有会话已保留。",
                ))
            }
        };
        if !self.current(generation) {
            return Err("登录检查已取消。".into());
        }
        let window = site_sessions::login_window(app, platform)?;
        state
            .site_sessions
            .restore_missing(app, platform, || self.current(generation))
            .await?;
        let saved = state.site_sessions.cookie(platform);
        let live = site_sessions::capture(app, platform).await?;
        if !has_account_cookie(platform, &saved) && !has_account_cookie(platform, &live) {
            return Ok(LoginCheck::new(
                "loggedout",
                "尚未保存此平台的登录状态，请先扫码登录。",
            ));
        }
        window
            .navigate(platform.home().parse().unwrap())
            .map_err(|_| "无法检查平台登录状态。")?;
        let result = async {
            let deadline = tokio::time::Instant::now() + Duration::from_secs(7);
            let started = tokio::time::Instant::now();
            let mut evidence=CheckEvidence::default();
            for _ in 0..32 {
                if tokio::time::Instant::now() >= deadline {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(150)).await;
                if !self.current(generation) {
                    return Err("登录检查已取消。".into());
                }
                let value = match frame(app, platform,true).await {
                    Ok(value) => value,
                    Err(_) => {evidence.observe("loading",started.elapsed());continue;},
                };
                if evidence.observe(&value.state,started.elapsed()) {
                    self.complete(app, platform, generation).await?;
                    self.checked.lock().unwrap().insert(platform,(state.site_sessions.cookie(platform),Instant::now()));
                    return Ok(LoginCheck::new("authenticated","登录状态有效。"));
                }
                if value.state == "verification" {
                    return Ok(LoginCheck::new("unknown","平台暂时要求验证，尚无法确认登录，已保留原有账号和会话，请稍后重试。"));
                }
            }
            Ok(LoginCheck::new("unknown","暂时无法确认登录状态，原有账号和会话已保留。请检查网络后重试；如解析也要求登录，可重新扫码。"))
        }
        .await;
        if self.current(generation) {
            let _ = window.navigate("about:blank".parse().unwrap());
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checks_require_stable_positive_account_evidence_never_qr_expiration() {
        let mut evidence = CheckEvidence::default();
        for state in [
            "loading",
            "waiting",
            "expired",
            "verification",
            "login-required",
        ] {
            assert!(!evidence.observe(state, Duration::from_secs(10)));
        }
        assert!(!evidence.observe("confirmed", Duration::from_millis(100)));
        assert!(!evidence.observe("confirmed", Duration::from_millis(300)));
        assert!(!evidence.observe("loading", Duration::from_secs(4)));
        assert!(!evidence.observe("confirmed", Duration::from_secs(5)));
        assert!(evidence.observe("confirmed", Duration::from_secs(6)));
    }
    #[test]
    fn qr_images_are_bounded_raster_data_not_remote_or_active_content() {
        assert!(!valid_image("https://example.test/qr.png"));
        assert!(!valid_image(
            "data:image/svg+xml;base64,PHN2Zz48c2NyaXB0Pjwvc2NyaXB0Pjwvc3ZnPg=="
        ));
        assert!(!valid_image("data:image/png;base64,ZmFrZQ=="));
        use base64::Engine;
        let image = format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD
                .encode([b"\x89PNG\r\n\x1a\n".as_slice(), &[0u8; 100]].concat())
        );
        assert!(valid_image(&image));
    }
    #[test]
    fn anonymous_or_other_platform_cookies_do_not_confirm_account_login() {
        assert!(!has_account_cookie(
            Platform::Douyin,
            "ttwid=anonymous;passport_csrf_token=fake"
        ));
        assert!(!has_account_cookie(
            Platform::Xiaohongshu,
            "a1=anonymous; webId=fake"
        ));
        assert!(!has_account_cookie(
            Platform::Douyin,
            "web_session=other-platform"
        ));
        assert!(has_account_cookie(
            Platform::Douyin,
            "sessionid=fake-official"
        ));
        assert!(has_account_cookie(
            Platform::Xiaohongshu,
            "a1=anonymous; web_session=fake-official"
        ));
    }
}
