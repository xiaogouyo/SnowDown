use crate::{
    auth,
    model::{now_ms, Platform},
    site_sessions, AppState,
};
use serde::{Deserialize, Serialize};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
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
                    Platform::Bilibili => false,
                }
        })
}

async fn frame(app: &AppHandle, platform: Platform) -> Result<Frame, String> {
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
        .replace("__PLATFORM__", &serde_json::to_string(&platform).unwrap());
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let sender = Mutex::new(Some(sender));
    window
        .eval_with_callback(script, move |value| {
            if let Some(sender) = sender.lock().unwrap().take() {
                let _ = sender.send(value);
            }
        })
        .map_err(|_| "官方二维码读取失败。")?;
    let raw = tokio::time::timeout(Duration::from_secs(3), receiver)
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
        let account = frame(app, platform)
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
            let value = match frame(app, platform).await {
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
        let value = frame(app, platform).await?;
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

    pub async fn check(&self, app: &AppHandle, platform: Platform) -> Result<bool, String> {
        self.cancel(app, platform);
        let generation = {
            let mut inner = self.inner.lock().unwrap();
            inner.platform = Some(platform);
            inner.generation += 1;
            inner.pending = None;
            inner.generation
        };
        let window = site_sessions::login_window(app, platform)?;
        window
            .navigate(platform.home().parse().unwrap())
            .map_err(|_| "无法检查平台登录状态。")?;
        let result = async {
            let deadline = tokio::time::Instant::now() + Duration::from_secs(18);
            let started = tokio::time::Instant::now();
            let mut confirmed_frames = 0;
            for _ in 0..25 {
                if tokio::time::Instant::now() >= deadline {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(600)).await;
                if !self.current(generation) {
                    return Err("登录检查已取消。".into());
                }
                let value = match frame(app, platform).await {
                    Ok(value) => value,
                    Err(_) => continue,
                };
                if value.state == "confirmed" {
                    confirmed_frames += 1;
                    if started.elapsed() < Duration::from_secs(3) || confirmed_frames < 2 {
                        continue;
                    }
                    self.complete(app, platform, generation).await?;
                    return Ok(true);
                }
                confirmed_frames = 0;
                if valid_image(&value.image) || value.state == "expired" {
                    let inner = self.inner.lock().unwrap();
                    if inner.generation != generation {
                        return Err("登录检查已取消。".into());
                    }
                    app.state::<Arc<AppState>>().site_sessions.clear(platform)?;
                    return Ok(false);
                }
                if value.state == "verification" {
                    return Err("平台暂时要求验证，无法确认登录状态，请稍后重试。".into());
                }
            }
            Err("暂时无法确认登录状态，已保留保存的账号，请检查网络后重试。".into())
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
