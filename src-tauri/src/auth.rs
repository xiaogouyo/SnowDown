//! Official Bilibili web QR login. Credentials never cross the IPC boundary.
use crate::model::now_ms;
use base64::{engine::general_purpose::STANDARD, Engine};
use reqwest::header::{HeaderMap, SET_COOKIE};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, path::PathBuf, sync::Mutex};

const PASSPORT: &str = "https://passport.bilibili.com/x/passport-login/web/qrcode";

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub uid: u64,
    pub name: String,
    pub avatar: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthStatus {
    pub state: String,
    pub account: Option<Account>,
    pub message: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Credentials {
    cookie: String,
    refresh_token: String,
    account: Option<Account>,
}

struct Pending {
    id: String,
    key: String,
    expires_at: u64,
}

struct Inner {
    credentials: Option<Credentials>,
    status: AuthStatus,
    pending: Option<Pending>,
    generation: u64,
}

pub struct Auth {
    path: PathBuf,
    inner: Mutex<Inner>,
    client: reqwest::Client,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginQr {
    id: String,
    image: String,
    expires_at: u64,
}

#[derive(Serialize)]
pub struct LoginPoll {
    pub state: &'static str,
}

fn status(state: &str, account: Option<Account>, message: Option<String>) -> AuthStatus {
    AuthStatus {
        state: state.into(),
        account,
        message,
    }
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/131.0.0.0 Safari/537.36")
        .default_headers({
            let mut headers = HeaderMap::new();
            headers.insert(reqwest::header::REFERER, "https://www.bilibili.com/".parse().unwrap());
            headers
        })
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(20))
        .build().expect("TLS client configuration")
}

impl Auth {
    pub fn open(path: PathBuf) -> Self {
        let (credentials, state) = match load(&path) {
            Ok(Some(c)) => {
                let state = status("checking", c.account.clone(), None);
                (Some(c), state)
            }
            Ok(None) => (None, status("loggedout", None, None)),
            Err(_) => (
                None,
                status(
                    "loggedout",
                    None,
                    Some("无法读取已保存的登录状态，请重新扫码登录。".into()),
                ),
            ),
        };
        Self {
            path,
            inner: Mutex::new(Inner {
                credentials,
                status: state,
                pending: None,
                generation: 0,
            }),
            client: client(),
        }
    }

    pub fn cookie(&self) -> String {
        self.inner
            .lock()
            .unwrap()
            .credentials
            .as_ref()
            .map(|c| c.cookie.clone())
            .unwrap_or_default()
    }

    pub fn snapshot(&self) -> AuthStatus {
        self.inner.lock().unwrap().status.clone()
    }

    pub async fn begin(&self) -> Result<LoginQr, String> {
        let generation = {
            let mut inner = self.inner.lock().unwrap();
            inner.generation += 1;
            inner.pending = None;
            inner.generation
        };
        let value: Value = self
            .client
            .get(format!("{PASSPORT}/generate"))
            .send()
            .await
            .map_err(|_| "无法连接B站登录服务，请检查网络后重试。")?
            .error_for_status()
            .map_err(|_| "B站登录服务暂时不可用。")?
            .json()
            .await
            .map_err(|_| "B站登录服务返回了无效数据。")?;
        if value["code"].as_i64() != Some(0) {
            return Err("B站暂时无法生成二维码，请稍后重试。".into());
        }
        let url = value["data"]["url"]
            .as_str()
            .ok_or("登录二维码缺少地址。")?;
        let parsed = url::Url::parse(url).map_err(|_| "登录二维码地址无效。")?;
        if parsed.scheme() != "https"
            || !parsed
                .host_str()
                .is_some_and(|h| h == "bilibili.com" || h.ends_with(".bilibili.com"))
        {
            return Err("登录二维码不是B站官方地址。".into());
        }
        let key = value["data"]["qrcode_key"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("登录二维码缺少标识。")?;
        let svg = qrcode::QrCode::new(url.as_bytes())
            .map_err(|_| "无法生成登录二维码。")?
            .render::<qrcode::render::svg::Color>()
            .min_dimensions(220, 220)
            .build();
        let id = uuid::Uuid::new_v4().to_string();
        let expires_at = now_ms() + 180_000;
        let mut inner = self.inner.lock().unwrap();
        if inner.generation != generation {
            return Err("登录请求已取消。".into());
        }
        inner.pending = Some(Pending {
            id: id.clone(),
            key: key.into(),
            expires_at,
        });
        Ok(LoginQr {
            id,
            image: format!("data:image/svg+xml;base64,{}", STANDARD.encode(svg)),
            expires_at,
        })
    }

    pub fn cancel(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.generation += 1;
        inner.pending = None;
    }

    pub async fn poll(&self, id: &str) -> Result<LoginPoll, String> {
        let (key, generation) = {
            let inner = self.inner.lock().unwrap();
            let pending = inner
                .pending
                .as_ref()
                .filter(|p| p.id == id)
                .ok_or("登录请求已取消，请刷新二维码。")?;
            if pending.expires_at <= now_ms() {
                return Ok(LoginPoll { state: "expired" });
            }
            (pending.key.clone(), inner.generation)
        };
        let response = self
            .client
            .get(format!("{PASSPORT}/poll"))
            .query(&[("qrcode_key", key)])
            .send()
            .await
            .map_err(|_| "查询登录状态失败，请检查网络后重试。")?
            .error_for_status()
            .map_err(|_| "B站登录服务暂时不可用。")?;
        let headers = response.headers().clone();
        let value: Value = response.json().await.map_err(|_| "B站登录状态数据无效。")?;
        if value["code"].as_i64() != Some(0) {
            return Err("B站登录服务暂时不可用，请稍后重试。".into());
        }
        let state = poll_state(value["data"]["code"].as_i64())?;
        if state != "confirmed" {
            let mut inner = self.inner.lock().unwrap();
            if inner.generation != generation {
                return Err("登录请求已取消。".into());
            }
            if state == "expired" {
                inner.pending = None;
            }
            return Ok(LoginPoll { state });
        }
        let cookie = login_cookie(&headers)?;
        let account = self
            .account(&cookie)
            .await
            .map_err(|_| "B站已确认扫码，但账号验证失败，请刷新二维码重试。")?
            .ok_or("B站登录状态未生效，请重新扫码。")?;
        let credentials = Credentials {
            cookie,
            refresh_token: value["data"]["refresh_token"]
                .as_str()
                .unwrap_or_default()
                .into(),
            account: Some(account.clone()),
        };
        self.complete(generation, credentials)?;
        Ok(LoginPoll { state: "confirmed" })
    }

    fn complete(&self, generation: u64, credentials: Credentials) -> Result<(), String> {
        let mut inner = self.inner.lock().unwrap();
        if inner.generation != generation {
            return Err("登录请求已取消。".into());
        }
        // Publish a successful login only after its encrypted file is durable.
        save(&self.path, &credentials)?;
        inner.status = status("authenticated", credentials.account.clone(), None);
        inner.credentials = Some(credentials);
        inner.pending = None;
        inner.generation += 1;
        Ok(())
    }

    async fn account(&self, cookie: &str) -> Result<Option<Account>, String> {
        let value: Value = self
            .client
            .get("https://api.bilibili.com/x/web-interface/nav")
            .header(reqwest::header::COOKIE, cookie)
            .send()
            .await
            .map_err(|_| "网络连接失败。")?
            .error_for_status()
            .map_err(|_| "账号服务不可用。")?
            .json()
            .await
            .map_err(|_| "账号数据无效。")?;
        if value["code"].as_i64() == Some(-101) || value["data"]["isLogin"].as_bool() == Some(false)
        {
            return Ok(None);
        }
        if value["code"].as_i64() != Some(0) || value["data"]["isLogin"].as_bool() != Some(true) {
            return Err("账号服务暂时不可用。".into());
        }
        Ok(Some(Account {
            uid: value["data"]["mid"].as_u64().ok_or("账号标识无效。")?,
            name: value["data"]["uname"].as_str().unwrap_or("B站用户").into(),
            avatar: value["data"]["face"].as_str().unwrap_or_default().into(),
        }))
    }

    pub async fn validate(&self) {
        let cookie = {
            let inner = self.inner.lock().unwrap();
            let Some(c) = &inner.credentials else { return };
            c.cookie.clone()
        };
        let result = self.account(&cookie).await;
        self.finish_validation(&cookie, result);
    }

    fn finish_validation(&self, cookie: &str, result: Result<Option<Account>, String>) {
        let mut inner = self.inner.lock().unwrap();
        // Cancelling a QR does not cancel startup validation. Changing the account does.
        if inner.credentials.as_ref().map(|c| c.cookie.as_str()) != Some(cookie) {
            return;
        }
        match result {
            Ok(Some(account)) => {
                if let Some(c) = inner.credentials.as_mut() {
                    c.account = Some(account.clone());
                }
                inner.status = status("authenticated", Some(account), None);
            }
            Ok(None) => {
                inner.status = status("expired", None, Some("B站登录已失效，请重新扫码。".into()));
                inner.credentials = None;
                let _ = std::fs::remove_file(&self.path);
            }
            Err(_) => {
                inner.status.state = "offline".into();
                inner.status.message =
                    Some("暂时无法验证账号，已保留本机登录状态。联网后可重试。".into());
            }
        }
    }

    pub fn logout(&self) -> Result<(), String> {
        let mut inner = self.inner.lock().unwrap();
        match std::fs::remove_file(&self.path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err("无法删除已保存的登录状态，请检查文件权限后重试。".into()),
        }
        inner.generation += 1;
        inner.pending = None;
        inner.credentials = None;
        inner.status = status("loggedout", None, None);
        Ok(())
    }
}

fn poll_state(code: Option<i64>) -> Result<&'static str, String> {
    match code {
        Some(86101) => Ok("waiting"),
        Some(86090) => Ok("scanned"),
        Some(86038) => Ok("expired"),
        Some(0) => Ok("confirmed"),
        _ => Err("B站返回了未识别的登录状态，请刷新二维码。".into()),
    }
}

fn login_cookie(headers: &HeaderMap) -> Result<String, String> {
    let mut pairs = BTreeMap::new();
    for value in headers.get_all(SET_COOKIE) {
        let value = value.to_str().map_err(|_| "B站返回了无效登录凭据。")?;
        if let Some((key, value)) = value.split(';').next().unwrap_or_default().split_once('=') {
            if matches!(
                key.trim(),
                "SESSDATA"
                    | "bili_jct"
                    | "DedeUserID"
                    | "DedeUserID__ckMd5"
                    | "sid"
                    | "buvid3"
                    | "buvid4"
            ) && !value.is_empty()
            {
                pairs.insert(key.trim().to_string(), value.to_string());
            }
        }
    }
    if !pairs.contains_key("SESSDATA") || !pairs.contains_key("DedeUserID") {
        return Err("B站没有返回完整登录凭据，请重新扫码。".into());
    }
    Ok(pairs
        .into_iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("; "))
}

fn load(path: &std::path::Path) -> Result<Option<Credentials>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(path).map_err(|_| "无法读取登录状态。")?;
    if bytes.len() > 64 * 1024 {
        return Err("登录状态文件无效。".into());
    }
    let plain = crypt(&bytes, false)?;
    let c: Credentials = serde_json::from_slice(&plain).map_err(|_| "登录状态文件无效。")?;
    if c.cookie.is_empty() || c.cookie.chars().any(char::is_control) {
        return Err("登录状态文件无效。".into());
    }
    Ok(Some(c))
}

fn save(path: &std::path::Path, credentials: &Credentials) -> Result<(), String> {
    let bytes = serde_json::to_vec(credentials).map_err(|_| "无法保存登录状态。")?;
    let encrypted = crypt(&bytes, true)?;
    let temp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| -> std::io::Result<()> {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(&encrypted)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result.map_err(|_| "无法持久化登录状态，请检查磁盘空间和文件权限。".into())
}

#[cfg(windows)]
pub(crate) fn crypt(bytes: &[u8], encrypt: bool) -> Result<Vec<u8>, String> {
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{
            CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        },
    };
    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes.len().try_into().map_err(|_| "登录状态过大。")?,
        pbData: bytes.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    // User-scoped DPAPI. Never enable CRYPTPROTECT_LOCAL_MACHINE.
    unsafe {
        let success = if encrypt {
            CryptProtectData(
                &input,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        if success == 0 {
            return Err("Windows 无法加密或读取登录状态，请重新扫码。".into());
        }
        let result = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        LocalFree(output.pbData as *mut _);
        Ok(result)
    }
}

#[cfg(not(windows))]
pub(crate) fn crypt(_: &[u8], _: bool) -> Result<Vec<u8>, String> {
    Err("当前版本的登录持久化仅支持 Windows。".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn official_qr_states() {
        assert_eq!(poll_state(Some(86101)).unwrap(), "waiting");
        assert_eq!(poll_state(Some(86090)).unwrap(), "scanned");
        assert_eq!(poll_state(Some(86038)).unwrap(), "expired");
        assert_eq!(poll_state(Some(0)).unwrap(), "confirmed");
        assert!(poll_state(Some(42)).is_err());
    }
    #[test]
    fn cookies_preserve_encoded_values_and_ignore_unrelated_headers() {
        let mut headers = HeaderMap::new();
        for c in [
            "SESSDATA=fake%2Cabc==; Expires=Wed, 01 Jan 2030 00:00:00 GMT; Secure",
            "DedeUserID=123; Path=/",
            "tracking=secret; Path=/",
        ] {
            headers.append(SET_COOKIE, c.parse().unwrap());
        }
        assert_eq!(
            login_cookie(&headers).unwrap(),
            "DedeUserID=123; SESSDATA=fake%2Cabc=="
        );
        assert!(login_cookie(&HeaderMap::new()).is_err());
    }
    #[test]
    #[cfg(windows)]
    fn encrypted_login_survives_reopen_and_logout_and_rejects_stale_completion() {
        let path = std::env::temp_dir().join(format!("snowdown-auth-{}.dat", uuid::Uuid::new_v4()));
        let auth = Auth::open(path.clone());
        let c = Credentials {
            cookie: "SESSDATA=fake-sensitive-test; DedeUserID=123".into(),
            refresh_token: "fake-refresh".into(),
            account: None,
        };
        auth.complete(0, c.clone()).unwrap();
        // A second successful save must atomically replace the previous file on Windows.
        auth.complete(1, c.clone()).unwrap();
        let saved = std::fs::read(&path).unwrap();
        assert!(!String::from_utf8_lossy(&saved).contains("fake-sensitive-test"));
        let reopened = Auth::open(path.clone());
        assert_eq!(reopened.cookie(), c.cookie);
        assert_eq!(reopened.snapshot().state, "checking");
        reopened.logout().unwrap();
        assert!(!path.exists());
        assert!(reopened.complete(0, c).is_err());
        assert!(reopened.cookie().is_empty());
        std::fs::write(&path, b"corrupt data").unwrap();
        assert!(Auth::open(path.clone()).cookie().is_empty());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    #[cfg(windows)]
    fn validation_keeps_saved_credentials_when_offline_and_cannot_undo_logout() {
        let path =
            std::env::temp_dir().join(format!("snowdown-offline-{}.dat", uuid::Uuid::new_v4()));
        let auth = Auth::open(path.clone());
        let cookie = "SESSDATA=fake-offline; DedeUserID=123";
        auth.complete(
            0,
            Credentials {
                cookie: cookie.into(),
                refresh_token: String::new(),
                account: None,
            },
        )
        .unwrap();
        auth.cancel();
        auth.finish_validation(cookie, Err("network unavailable".into()));
        assert_eq!(auth.snapshot().state, "offline");
        assert_eq!(Auth::open(path.clone()).cookie(), cookie);
        auth.logout().unwrap();
        auth.finish_validation(
            cookie,
            Ok(Some(Account {
                uid: 123,
                name: "test".into(),
                avatar: String::new(),
            })),
        );
        assert_eq!(auth.snapshot().state, "loggedout");
        assert!(!path.exists());
    }
    #[tokio::test]
    #[ignore = "contacts official Bilibili login service; generates an unused QR only"]
    async fn live_official_qr_waiting() {
        let auth =
            Auth::open(std::env::temp_dir().join(format!("unused-{}.dat", uuid::Uuid::new_v4())));
        let qr = auth.begin().await.unwrap();
        assert!(qr.image.starts_with("data:image/svg+xml;base64,"));
        assert_eq!(auth.poll(&qr.id).await.unwrap().state, "waiting");
        auth.cancel();
        assert!(auth.poll(&qr.id).await.is_err());
    }
}
