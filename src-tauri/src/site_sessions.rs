use crate::{auth, model::Platform, sites};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

fn cached_quality(item: &sites::Resolved, quality: u32) -> bool {
    item.quality == quality
        || (quality == 0 && item.info.qualities.iter().map(|q| q.id).max() == Some(item.quality))
}
async fn first_success<T>(
    first: impl std::future::Future<Output = Result<T, String>>,
    second: impl std::future::Future<Output = Result<T, String>>,
) -> Result<T, String> {
    tokio::pin!(first);
    tokio::pin!(second);
    tokio::select! {
        result=&mut first=>match result {Ok(value)=>Ok(value),Err(_)=>second.await},
        result=&mut second=>match result {Ok(value)=>Ok(value),Err(error)=>first.await.or(Err(error))},
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStatus {
    pub platform: Platform,
    pub saved: bool,
    pub account: Option<Profile>,
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub avatar: String,
}
fn auth_cookie_name(name: &str) -> bool {
    matches!(
        name,
        "web_session"
            | "web_session_ssl"
            | "sessionid"
            | "sessionid_ss"
            | "sid_tt"
            | "sid_guard"
            | "SESSDATA"
    )
}
fn cookie_pairs(value: &str) -> Vec<(&str, &str)> {
    value
        .split(';')
        .filter_map(|part| part.trim().split_once('='))
        .filter(|(key, value)| {
            !key.is_empty()
                && key.len() <= 256
                && !value.chars().any(char::is_control)
                && key
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
        })
        .collect()
}
fn restore_plan(
    platform: Platform,
    saved: &str,
    live: &str,
) -> Vec<tauri::webview::Cookie<'static>> {
    let domain = if platform == Platform::Douyin {
        ".douyin.com"
    } else {
        ".xiaohongshu.com"
    };
    let existing = cookie_pairs(live);
    cookie_pairs(saved)
        .into_iter()
        .filter(|(key, _)| !existing.iter().any(|(current, _)| current == key))
        .map(|(key, value)| {
            tauri::webview::Cookie::build((key.to_string(), value.to_string()))
                .domain(domain)
                .path("/")
                .secure(true)
                .http_only(auth_cookie_name(key))
                .build()
        })
        .collect()
}
impl Profile {
    pub fn sanitized(self) -> Self {
        let avatar = url::Url::parse(if self.avatar.len() <= 4096 {
            &self.avatar
        } else {
            ""
        })
        .ok()
        .filter(|url| {
            url.scheme() == "https"
                && url.username().is_empty()
                && url.password().is_none()
                && url.port().is_none()
                && url.host_str().is_some_and(|host| {
                    [
                        "douyinpic.com",
                        "douyin.com",
                        "byteimg.com",
                        "bytecdn.cn",
                        "ibytedtos.com",
                        "xhscdn.com",
                        "xiaohongshu.com",
                    ]
                    .iter()
                    .any(|suffix| sites::host_matches(host, suffix))
                })
        })
        .map(|url| url.to_string())
        .unwrap_or_default();
        Self {
            name: self
                .name
                .chars()
                .filter(|c| !c.is_control())
                .take(80)
                .collect(),
            avatar,
        }
    }
}

pub struct Sessions {
    directory: PathBuf,
    cookies: Mutex<HashMap<Platform, String>>,
    profiles: Mutex<HashMap<Platform, Profile>>,
    media: Mutex<HashMap<(Platform, String), (Instant, sites::Resolved)>>,
    observed: Mutex<Observed>,
    parsers: [tokio::sync::Mutex<()>; 2],
    resolvers: [tokio::sync::Mutex<()>; 2],
}
#[derive(Default)]
struct Observed {
    responses: u64,
    errors: u64,
    details: HashMap<String, (Instant, serde_json::Value)>,
}

impl Sessions {
    pub async fn checking_guard(
        &self,
        platform: Platform,
    ) -> Result<tokio::sync::MutexGuard<'_, ()>, String> {
        let index = match platform {
            Platform::Douyin => 0,
            Platform::Xiaohongshu => 1,
            _ => return Err("B站使用专用登录模块。".into()),
        };
        Ok(self.parsers[index].lock().await)
    }
    pub async fn restore_missing(
        &self,
        app: &AppHandle,
        platform: Platform,
        current: impl Fn() -> bool,
    ) -> Result<(), String> {
        let saved = self.cookie(platform);
        if saved.is_empty() {
            return Ok(());
        }
        let window = login_window(app, platform)?;
        let live = capture(app, platform).await?;
        if !current() {
            return Err("登录检查已取消。".into());
        }
        crate::web_capture::restore_cookies(&window, restore_plan(platform, &saved, &live)).await
    }
    pub fn open(directory: PathBuf) -> Self {
        let mut cookies = HashMap::new();
        let mut profiles = HashMap::new();
        for platform in [Platform::Douyin, Platform::Xiaohongshu] {
            let path = directory.join(format!("{}.dat", platform.key()));
            if std::fs::metadata(&path)
                .map(|m| m.len() > 128 * 1024)
                .unwrap_or(true)
            {
                continue;
            }
            if let Ok(bytes) = std::fs::read(path) {
                if let Ok(plain) = auth::crypt(&bytes, false) {
                    if let Ok(cookie) = String::from_utf8(plain) {
                        if !cookie.is_empty()
                            && cookie.len() <= 64 * 1024
                            && !cookie.chars().any(char::is_control)
                        {
                            cookies.insert(platform, cookie);
                        }
                    }
                }
            }
        }
        for platform in [Platform::Douyin, Platform::Xiaohongshu] {
            if cookies.contains_key(&platform) {
                if let Ok(bytes) =
                    std::fs::read(directory.join(format!("{}.profile.dat", platform.key())))
                {
                    if bytes.len() < 16 * 1024 {
                        if let Ok(plain) = auth::crypt(&bytes, false) {
                            if let Ok(profile) = serde_json::from_slice::<Profile>(&plain) {
                                profiles.insert(platform, profile.sanitized());
                            }
                        }
                    }
                }
            }
        }
        Self {
            directory,
            cookies: Mutex::new(cookies),
            profiles: Mutex::new(profiles),
            media: Mutex::new(HashMap::new()),
            observed: Mutex::new(Observed::default()),
            parsers: [tokio::sync::Mutex::new(()), tokio::sync::Mutex::new(())],
            resolvers: [tokio::sync::Mutex::new(()), tokio::sync::Mutex::new(())],
        }
    }
    pub fn cookie(&self, platform: Platform) -> String {
        self.cookies
            .lock()
            .unwrap()
            .get(&platform)
            .cloned()
            .unwrap_or_default()
    }
    pub fn remember_response(&self, source: &sites::Source, bytes: &[u8]) {
        let mut observed = self.observed.lock().unwrap();
        observed.responses += 1;
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(bytes) else {
            observed.errors += 1;
            return;
        };
        let Ok(expected) = sites::video_id(source) else {
            return;
        };
        fn matching<'a>(
            value: &'a serde_json::Value,
            id: &str,
            source: &sites::Source,
            depth: usize,
            visited: &mut usize,
        ) -> Option<&'a serde_json::Value> {
            if depth > 18 || *visited >= 18000 {
                return None;
            }
            *visited += 1;
            let matches_id = value["aweme_id"].as_str() == Some(id)
                || value["aweme_id"]
                    .as_u64()
                    .is_some_and(|number| number.to_string() == id);
            if matches_id
                && value["video"].is_object()
                && sites::from_detail(
                    source,
                    &serde_json::json!({"aweme_detail":{"aweme_id":id,"video":value["video"]}}),
                    0,
                )
                .is_ok()
            {
                return Some(value);
            }
            match value {
                serde_json::Value::Array(items) => items
                    .iter()
                    .take(300)
                    .find_map(|item| matching(item, id, source, depth + 1, visited)),
                serde_json::Value::Object(items) => items
                    .values()
                    .take(300)
                    .find_map(|item| matching(item, id, source, depth + 1, visited)),
                _ => None,
            }
        }
        let Some(detail) = matching(&value, &expected, source, 0, &mut 0) else {
            return;
        };
        let video = &detail["video"];
        #[cfg(debug_assertions)]
        if std::env::var_os("SNOWDOWN_TEST_SOURCE").is_some() {
            fn shape(value: &serde_json::Value, depth: usize) -> serde_json::Value {
                if depth > 8 {
                    return "nested".into();
                }
                match value {
                    serde_json::Value::Object(map) => serde_json::Value::Object(
                        map.iter()
                            .map(|(k, v)| (k.clone(), shape(v, depth + 1)))
                            .collect(),
                    ),
                    serde_json::Value::Array(values) => serde_json::Value::Array(
                        values
                            .first()
                            .map(|v| vec![shape(v, depth + 1)])
                            .unwrap_or_default(),
                    ),
                    serde_json::Value::String(value) => {
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(value) {
                            shape(&json, depth + 1)
                        } else {
                            "string".into()
                        }
                    }
                    serde_json::Value::Number(_) => "number".into(),
                    serde_json::Value::Bool(_) => "boolean".into(),
                    serde_json::Value::Null => serde_json::Value::Null,
                }
            }
            let _ = std::fs::write(
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.tools/v012-video-schema.json"),
                serde_json::to_vec_pretty(&shape(video, 0)).unwrap_or_default(),
            );
        }
        let pared = serde_json::json!({"aweme_detail":{
            "aweme_id": expected, "desc": detail["desc"], "author":{"nickname":detail["author"]["nickname"]},
            "video": {"width":video["width"],"height":video["height"],"duration":video["duration"],"cover":video["cover"],
                "play_addr":video["play_addr"],"play_addr_h264":video["play_addr_h264"],"play_addr_bytevc1":video["play_addr_bytevc1"],
                "bit_rate":video["bit_rate"],"bit_rate_audio":video["bit_rate_audio"]}
        }});
        // Incomplete feed items must not poison the cache and short-circuit a
        // subsequent valid HTTP/page result for the next five minutes.
        if sites::from_detail(source, &pared, 0).is_err() {
            return;
        }
        observed
            .details
            .retain(|_, (time, _)| time.elapsed() < Duration::from_secs(300));
        if observed.details.len() >= 100 {
            observed.details.clear();
        }
        observed.details.insert(expected, (Instant::now(), pared));
    }
    fn observed_video(
        &self,
        source: &sites::Source,
        quality: u32,
    ) -> Option<Result<sites::Resolved, String>> {
        if source.platform != Platform::Douyin {
            return None;
        }
        let observed = self.observed.lock().unwrap();
        let (_, value) = observed
            .details
            .get(&sites::video_id(source).ok()?)
            .filter(|(time, _)| time.elapsed() < Duration::from_secs(300))?;
        Some(sites::from_detail(source, value, quality))
    }
    pub fn snapshot(&self) -> Vec<SessionStatus> {
        let cookies = self.cookies.lock().unwrap();
        [Platform::Douyin, Platform::Xiaohongshu]
            .into_iter()
            .map(|platform| SessionStatus {
                platform,
                saved: cookies.contains_key(&platform),
                account: self.profiles.lock().unwrap().get(&platform).cloned(),
            })
            .collect()
    }
    pub fn remember_browser(
        &self,
        source: &sites::Source,
        detail: &serde_json::Value,
        quality: u32,
    ) -> Result<sites::Resolved, String> {
        let resolved = sites::from_detail(source, detail, quality)?;
        let id = sites::video_id(source)?;
        if resolved.info.bvid != id {
            return Err("浏览器视频与目标链接不一致。".into());
        }
        let mut media = self.media.lock().unwrap();
        media.retain(|_, (time, _)| time.elapsed() < Duration::from_secs(300));
        if media.len() >= 100 {
            media.clear();
        }
        media.insert((source.platform, id), (Instant::now(), resolved.clone()));
        Ok(resolved)
    }
    pub fn save(&self, platform: Platform, cookie: String) -> Result<(), String> {
        if platform == Platform::Bilibili {
            return Err("B站请使用官方二维码登录。".into());
        }
        if cookie.is_empty() || cookie.len() > 64 * 1024 || cookie.chars().any(char::is_control) {
            return Err("未取得有效网页会话，请先在官方页面完成登录或验证。".into());
        }
        if self.cookie(platform) == cookie
            && self
                .directory
                .join(format!("{}.dat", platform.key()))
                .is_file()
        {
            return Ok(());
        }
        let bytes = auth::crypt(cookie.as_bytes(), true)?;
        std::fs::create_dir_all(&self.directory).map_err(|_| "无法创建会话目录。")?;
        let path = self.directory.join(format!("{}.dat", platform.key()));
        let temp = self.directory.join(format!("{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| -> std::io::Result<()> {
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            std::fs::rename(&temp, &path)
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(temp);
            return Err("会话保存失败，请检查文件权限。".into());
        }
        self.cookies.lock().unwrap().insert(platform, cookie);
        self.media
            .lock()
            .unwrap()
            .retain(|(site, _), _| *site != platform);
        Ok(())
    }
    pub fn clear(&self, platform: Platform) -> Result<(), String> {
        let path = self.directory.join(format!("{}.dat", platform.key()));
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err("无法清除已保存会话。".into()),
        }
        self.cookies.lock().unwrap().remove(&platform);
        self.profiles.lock().unwrap().remove(&platform);
        let _ = std::fs::remove_file(
            self.directory
                .join(format!("{}.profile.dat", platform.key())),
        );
        if platform == Platform::Douyin {
            self.observed.lock().unwrap().details.clear();
        }
        self.media
            .lock()
            .unwrap()
            .retain(|(site, _), _| *site != platform);
        Ok(())
    }
    pub fn save_profile(&self, platform: Platform, profile: Profile) -> Result<(), String> {
        let profile = profile.sanitized();
        if profile.avatar.is_empty() && profile.name.is_empty() {
            return Ok(());
        }
        let bytes = auth::crypt(
            &serde_json::to_vec(&profile).map_err(|_| "账号信息无效。")?,
            true,
        )?;
        std::fs::create_dir_all(&self.directory).map_err(|_| "无法保存账号信息。")?;
        std::fs::write(
            self.directory
                .join(format!("{}.profile.dat", platform.key())),
            bytes,
        )
        .map_err(|_| "无法保存账号信息。")?;
        self.profiles.lock().unwrap().insert(platform, profile);
        Ok(())
    }
    pub async fn import_browser(
        &self,
        app: &AppHandle,
        platform: Platform,
        cookie: String,
    ) -> Result<(), String> {
        if cookie.is_empty() || cookie.len() > 64 * 1024 || cookie.chars().any(char::is_control) {
            return Err("浏览器会话无效。".into());
        }
        if platform == Platform::Bilibili {
            return Err("B站使用专用登录模块。".into());
        }
        let _guard = self.checking_guard(platform).await?;
        let window = login_window(app, platform)?;
        crate::web_capture::restore_cookies(&window, restore_plan(platform, &cookie, "")).await?;
        self.save(platform, cookie)
    }

    pub async fn logout(&self, app: &AppHandle, platform: Platform) -> Result<(), String> {
        let index = match platform {
            Platform::Douyin => 0,
            Platform::Xiaohongshu => 1,
            _ => return Err("B站使用专用登录模块。".into()),
        };
        let _guard = self.parsers[index].lock().await;
        let window = login_window(app, platform)?;
        window
            .navigate("about:blank".parse().unwrap())
            .map_err(|_| "无法停止平台登录页面。")?;
        if let Some(parser) = app.get_webview_window(&format!("resolver-{}", platform.key())) {
            let _ = parser.navigate("about:blank".parse().unwrap());
        }
        crate::web_capture::clear_profile(&window).await?;
        self.clear(platform)
    }

    pub async fn resolve(
        &self,
        app: &AppHandle,
        source: &sites::Source,
        quality: u32,
    ) -> Result<sites::Resolved, String> {
        let index = match source.platform {
            Platform::Douyin => 0,
            Platform::Xiaohongshu => 1,
            _ => return Err("B站使用专用解析器。".into()),
        };
        let _request = self.resolvers[index].lock().await;
        let canonical = sites::canonical_source(source, &self.cookie(source.platform)).await?;
        let source = &canonical;
        let key = (source.platform, sites::video_id(source)?);
        {
            let mut media = self.media.lock().unwrap();
            media.retain(|_, (time, _)| time.elapsed() < Duration::from_secs(300));
            if let Some((_, cached)) = media
                .get(&key)
                .filter(|(_, item)| cached_quality(item, quality))
            {
                return Ok(cached.clone());
            }
        }
        let result = if let Some(result) = self.observed_video(source, quality) {
            result?
        } else {
            let cookie = self.cookie(source.platform);
            // A blocked HTTP response must not consume 30 seconds before the
            // browser even starts. Both paths are bounded; only a success wins.
            first_success(
                async {
                    tokio::time::timeout(
                        Duration::from_secs(7),
                        sites::resolve(source, quality, &cookie),
                    )
                    .await
                    .unwrap_or_else(|_| Err("平台直连读取超时。".into()))
                },
                async {
                    tokio::time::sleep(Duration::from_millis(250)).await;
                    self.resolve_in_background(app, source, quality).await
                },
            )
            .await?
        };
        let result_key = (source.platform, result.info.bvid.clone());
        let mut media = self.media.lock().unwrap();
        if media.len() >= 100 {
            media.clear();
        }
        media.insert(result_key, (Instant::now(), result.clone()));
        Ok(result)
    }

    async fn resolve_in_background(
        &self,
        app: &AppHandle,
        source: &sites::Source,
        quality: u32,
    ) -> Result<sites::Resolved, String> {
        let index = match source.platform {
            Platform::Douyin => 0,
            Platform::Xiaohongshu => 1,
            Platform::Bilibili | Platform::Web => return Err("此平台使用专用解析器。".into()),
        };
        // A single page per platform shares its login profile. Serialize page
        // navigation so concurrent queue items cannot replace each other's source.
        let _guard = self.parsers[index].lock().await;
        if let Some(result) = self.observed_video(source, quality) {
            return result;
        }
        self.restore_missing(app, source.platform, || true).await?;
        let window = open_parser(app, source)?;
        struct Stop(tauri::WebviewWindow);
        impl Drop for Stop {
            fn drop(&mut self) {
                let _ = self.0.navigate("about:blank".parse().unwrap());
            }
        }
        let _stop = Stop(window.clone());
        let result = tokio::time::timeout(Duration::from_secs(18), async {
            loop {
                match read_video(app, source, quality).await {
                    Ok(resolved) => return Ok(resolved),
                    Err(error) if error.starts_with("平台安全验证：") => return Err(error),
                    Err(_) => {}
                }
                if window.url().ok().is_some_and(|url| url.path().contains("login")) {
                    return Err(format!("{}需要登录，请点击“登录{}”完成登录后，再点击“解析视频”。", source.platform.label(), source.platform.label()));
                }
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }).await.unwrap_or_else(|_| Err(format!("{}暂未返回此链接的视频详情。请确认链接有效；如需登录或验证，请在平台账号中完成后重新解析。", source.platform.label())));
        // Stop playback and network work once metadata is available; the login
        // window is never navigated, shown or used as the download source.
        let _ = window.navigate("about:blank".parse().unwrap());
        result
    }
}

pub fn login_window(app: &AppHandle, platform: Platform) -> Result<tauri::WebviewWindow, String> {
    if platform == Platform::Bilibili {
        return Err("B站请使用官方二维码登录。".into());
    }
    let label = format!("official-{}", platform.key());
    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.hide();
        return Ok(window);
    }
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|_| "应用数据目录不可用。")?
        .join("official-profiles")
        .join(platform.key());
    #[cfg(debug_assertions)]
    let directory = if std::env::var_os("SNOWDOWN_QR_VERIFY").is_some() {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../.tools/qr-verify-profiles")
            .join(platform.key())
    } else {
        directory
    };
    std::fs::create_dir_all(&directory).map_err(|_| "官方网页会话目录不可用。")?;
    let window = WebviewWindowBuilder::new(app, label, WebviewUrl::External("about:blank".parse().unwrap()))
        .title(format!("SnowDown · {}后台扫码登录",platform.label()))
        .inner_size(1100.0, 780.0)
        .min_inner_size(800.0, 600.0)
        .visible(false)
        .focused(false)
        .skip_taskbar(true)
        .initialization_script("document.addEventListener('play', event => { if(event.target instanceof HTMLMediaElement) {event.target.muted=true;event.target.pause();}},true)")
        .data_directory(directory)
        .build()
        .map_err(|e| format!("无法打开官方登录：{e}"))?;
    Ok(window)
}

fn open_parser(app: &AppHandle, source: &sites::Source) -> Result<tauri::WebviewWindow, String> {
    let label = format!("resolver-{}", source.platform.key());
    let destination = source.url.parse().map_err(|_| "视频链接无效。")?;
    if let Some(window) = app.get_webview_window(&label) {
        window
            .navigate(destination)
            .map_err(|_| "无法读取视频链接。")?;
        return Ok(window);
    }
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|_| "应用数据目录不可用。")?
        .join("official-profiles")
        .join(source.platform.key());
    std::fs::create_dir_all(&directory).map_err(|_| "平台登录目录不可用。")?;
    let window = WebviewWindowBuilder::new(
        app,
        label,
        WebviewUrl::External("about:blank".parse().unwrap()),
    )
    .title("SnowDown 后台解析")
    .initialization_script(include_str!("../../extension/page-hook.js"))
    .inner_size(1100.0, 780.0)
    .visible(false)
    .focused(false)
    .skip_taskbar(true)
    .data_directory(directory)
    .build()
    .map_err(|_| "后台解析组件启动失败。")?;
    crate::web_capture::attach(&window, destination)?;
    Ok(window)
}

fn diagnostics_path() -> Option<PathBuf> {
    std::env::var_os("SNOWDOWN_DIAGNOSTICS_PATH")
        .map(PathBuf::from)
        .or_else(|| {
            cfg!(debug_assertions).then(|| {
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../.tools/v012-site-diagnostics.json")
            })
        })
}

async fn read_video(
    app: &AppHandle,
    source: &sites::Source,
    quality: u32,
) -> Result<sites::Resolved, String> {
    let path = diagnostics_path();
    let result = read_video_inner(app, source, quality).await;
    if let Some(path) = &path {
        let mut diagnostic: serde_json::Value = tokio::fs::read(path)
            .await
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_else(|| serde_json::json!({}));
        let window = app.get_webview_window(&format!("resolver-{}", source.platform.key()));
        let actual = window.as_ref().and_then(|w| w.url().ok());
        diagnostic["windowExists"] = window.is_some().into();
        diagnostic["pageHost"] = actual
            .as_ref()
            .and_then(|u| u.host_str())
            .map(str::to_owned)
            .into();
        diagnostic["pagePath"] = actual.as_ref().map(|u| u.path().to_string()).into();
        diagnostic["requestedVideoId"] = sites::video_id(source).ok().into();
        diagnostic["error"] = result.as_ref().err().cloned().into();
        diagnostic["success"] = result.is_ok().into();
        let state = app.state::<std::sync::Arc<crate::AppState>>();
        {
            let observed = state.site_sessions.observed.lock().unwrap();
            diagnostic["nativeResponses"] = observed.responses.into();
            diagnostic["nativeJsonErrors"] = observed.errors.into();
            diagnostic["nativeVideos"] = observed.details.len().into();
        }
        let _ = tokio::fs::write(path, serde_json::to_vec(&diagnostic).unwrap_or_default()).await;
    }
    result
}

async fn read_video_inner(
    app: &AppHandle,
    source: &sites::Source,
    quality: u32,
) -> Result<sites::Resolved, String> {
    if let Some(result) = app
        .state::<std::sync::Arc<crate::AppState>>()
        .site_sessions
        .observed_video(source, quality)
    {
        return result;
    }
    let window = app
        .get_webview_window(&format!("resolver-{}", source.platform.key()))
        .ok_or("后台解析组件未启动。")?;
    let current = sites::recognize(window.url().map_err(|_| "无法读取官方页面。")?.as_str())?;
    if current.platform != source.platform || sites::video_id(&current)? != sites::video_id(source)?
    {
        return Err("后台页面尚未加载输入链接对应的视频。".into());
    }
    let script = include_str!("site_video.js")
        .replace(
            "__VIDEO_ID__",
            &serde_json::to_string(&sites::video_id(source)?).unwrap(),
        )
        .replace(
            "__CAPTURE_PAGE_SOURCE__",
            &include_str!("../../extension/page.js")
                .replace("export function capturePage", "function capturePage"),
        );
    let mut data: serde_json::Value;
    let (tx, rx) = tokio::sync::oneshot::channel();
    let sender = Mutex::new(Some(tx));
    window
        .eval_with_callback(script.clone(), move |value| {
            if let Some(sender) = sender.lock().unwrap().take() {
                let _ = sender.send(value);
            }
        })
        .map_err(|_| "读取网页视频失败。")?;
    let raw = tokio::time::timeout(Duration::from_millis(900), rx)
        .await
        .map_err(|_| "读取网页视频超时。")?
        .map_err(|_| "读取网页视频失败。")?;
    if raw.len() > 2 * 1024 * 1024 {
        return Err("网页数据过大。".into());
    }
    data = serde_json::from_str(&raw).map_err(|_| "网页视频数据无效。")?;
    if let Some(value) = data.as_str() {
        data = serde_json::from_str(value).map_err(|_| "网页视频数据无效。")?;
    }
    let result = if data["verificationRequired"].as_bool() == Some(true) {
        Err("平台安全验证：抖音返回了验证码页面，尚未提供视频数据。可在浏览器官方页面完成验证后，通过 SnowDown 插件读取并转发下载；或在平台账号中重新扫码后重试。".into())
    } else {
        sites::from_browser(source, &data, quality)
    };
    if let Some(path) = diagnostics_path() {
        // Opt-in developer diagnostics: shape and hostnames only. Never record
        // credentials, query parameters, page bodies, titles, or media URLs.
        fn hosts(value: &serde_json::Value, output: &mut std::collections::BTreeSet<String>) {
            match value {
                serde_json::Value::String(s) => {
                    if let Ok(url) = url::Url::parse(s) {
                        if let Some(host) = url.host_str() {
                            output.insert(host.to_string());
                        }
                    }
                }
                serde_json::Value::Array(values) => {
                    for value in values {
                        hosts(value, output);
                    }
                }
                serde_json::Value::Object(values) => {
                    for value in values.values() {
                        hosts(value, output);
                    }
                }
                _ => {}
            }
        }
        let mut media_hosts = std::collections::BTreeSet::new();
        hosts(&data["detail"]["video"], &mut media_hosts);
        let diagnostic = serde_json::json!({
            "platform": source.platform.key(), "success": result.is_ok(),
            "detailPresent": data["detail"].is_object(),
            "videoFields": data["detail"]["video"].as_object().map(|v| v.keys().collect::<Vec<_>>()),
            "mediaHosts": media_hosts,
            "blobPlayer": data["player"]["url"].as_str().is_some_and(|url| url.starts_with("blob:")),
            "captureStatus": data["captureStatus"], "readerError":data["readerError"],
            "verificationRequired": data["verificationRequired"]
        });
        let _ = tokio::fs::write(path, serde_json::to_vec(&diagnostic).unwrap_or_default()).await;
    }
    result
}

pub async fn capture(app: &AppHandle, platform: Platform) -> Result<String, String> {
    if platform == Platform::Bilibili {
        return Err("B站请使用官方二维码登录。".into());
    }
    let window = app
        .get_webview_window(&format!("official-{}", platform.key()))
        .ok_or("请先打开此平台的官方页面，并完成登录或验证。")?;
    let url = platform.home().parse().unwrap();
    let cookies = tokio::time::timeout(
        Duration::from_secs(5),
        tauri::async_runtime::spawn_blocking(move || window.cookies_for_url(url)),
    )
    .await
    .map_err(|_| "读取官方会话超时，请重新打开官方页面。")?
    .map_err(|_| "读取官方会话失败。")?
    .map_err(|_| "读取官方会话失败，请重新打开官方页面。")?;
    Ok(cookies
        .into_iter()
        .map(|cookie| format!("{}={}", cookie.name(), cookie.value()))
        .collect::<Vec<_>>()
        .join("; "))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_quality_reuses_only_highest_cached_result_and_unchanged_session_keeps_cache() {
        let directory =
            std::env::temp_dir().join(format!("snowdown-cache-{}", uuid::Uuid::new_v4()));
        let sessions = Sessions::open(directory.clone());
        let source = sites::recognize("https://www.douyin.com/video/1234567890123456789").unwrap();
        sessions
            .save(Platform::Douyin, "sessionid=fixture".into())
            .unwrap();
        let value = serde_json::json!({"aweme_detail":{"aweme_id":"1234567890123456789","video":{"width":1280,"height":720,"play_addr":{"url_list":["https://v.fixture.douyinvod.com/video.mp4"]}}}});
        let mut resolved = sessions.remember_browser(&source, &value, 0).unwrap();
        assert!(cached_quality(&resolved, 0));
        assert!(cached_quality(&resolved, 720));
        assert!(!cached_quality(&resolved, 480));
        resolved.quality = 480;
        assert!(!cached_quality(&resolved, 0));
        let previous = std::fs::read(directory.join("douyin.dat")).unwrap();
        sessions
            .save(Platform::Douyin, "sessionid=fixture".into())
            .unwrap();
        assert_eq!(sessions.media.lock().unwrap().len(), 1);
        assert_eq!(
            previous,
            std::fs::read(directory.join("douyin.dat")).unwrap()
        );
        sessions
            .save(Platform::Douyin, "sessionid=new-fixture".into())
            .unwrap();
        assert!(sessions.media.lock().unwrap().is_empty());
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[tokio::test]
    async fn fast_paths_race_success_not_first_error() {
        let start = Instant::now();
        let value = first_success(
            async {
                tokio::time::sleep(Duration::from_secs(1)).await;
                Ok::<_, String>(1)
            },
            async { Ok(2) },
        )
        .await
        .unwrap();
        assert_eq!(value, 2);
        assert!(start.elapsed() < Duration::from_millis(200));
        assert_eq!(
            first_success(async { Err("blocked".into()) }, async {
                tokio::time::sleep(Duration::from_millis(10)).await;
                Ok(3)
            })
            .await
            .unwrap(),
            3
        );
        assert!(
            first_success::<u8>(async { Err("one".into()) }, async { Err("two".into()) })
                .await
                .is_err()
        );
    }
    #[test]
    fn restored_cookies_keep_live_values_and_the_platform_script_tokens_visible() {
        let plan = restore_plan(
            Platform::Xiaohongshu,
            "web_session=saved; a1=old; csrfToken=saved-csrf",
            "a1=current; extra=current",
        );
        assert_eq!(plan.len(), 2);
        let session = plan.iter().find(|c| c.name() == "web_session").unwrap();
        assert_eq!(session.value(), "saved");
        assert_eq!(session.http_only(), Some(true));
        assert_eq!(session.path(), Some("/"));
        let csrf = plan.iter().find(|c| c.name() == "csrfToken").unwrap();
        assert_eq!(csrf.http_only(), Some(false));
        assert!(!plan.iter().any(|c| c.name() == "a1"));
        let dy = restore_plan(Platform::Douyin, "sessionid=auth; msToken=js-token", "");
        assert_eq!(
            dy.iter()
                .find(|c| c.name() == "sessionid")
                .unwrap()
                .http_only(),
            Some(true)
        );
        assert_eq!(
            dy.iter()
                .find(|c| c.name() == "msToken")
                .unwrap()
                .http_only(),
            Some(false)
        );
        assert!(
            restore_plan(Platform::Xiaohongshu, "web_session=old", "web_session=new").is_empty()
        );
    }
    #[test]
    fn observed_details_keep_only_matching_video_metadata_in_memory() {
        let sessions = Sessions::open(
            std::env::temp_dir().join(format!("snowdown-observed-{}", uuid::Uuid::new_v4())),
        );
        let source = sites::recognize("https://www.douyin.com/video/1234567890123456789").unwrap();
        let value = serde_json::json!({"aweme_detail":{"aweme_id":"1234567890123456789","desc":"视频","private_extra":"must-not-retain","author":{"nickname":"作者","uid":"must-not-retain"},"video":{"width":1280,"height":720,"play_addr":{"url_list":["https://www.douyin.com/aweme/v1/play/?video_id=fake"]}}}});
        sessions.remember_response(&source, &serde_json::to_vec(&value).unwrap());
        assert_eq!(
            sessions
                .observed_video(&source, 0)
                .unwrap()
                .unwrap()
                .quality,
            720
        );
        let observed = sessions.observed.lock().unwrap();
        assert!(!observed
            .details
            .values()
            .any(|(_, v)| v.to_string().contains("must-not-retain")));
        drop(observed);
        let other = sites::recognize("https://www.douyin.com/video/9876543210987654321").unwrap();
        sessions.remember_response(&other, &serde_json::to_vec(&value).unwrap());
        assert!(sessions.observed_video(&other, 0).is_none());
    }
    #[test]
    fn captured_feed_selects_exact_requested_video_without_caching_incomplete_items() {
        let sessions = Sessions::open(
            std::env::temp_dir().join(format!("snowdown-feed-{}", uuid::Uuid::new_v4())),
        );
        let source =
            sites::recognize("https://www.douyin.com/jingxuan?modal_id=1234567890123456789")
                .unwrap();
        let expected = sites::video_id(&source).unwrap();
        let empty = serde_json::json!({"aweme_list":[{"aweme_id":expected,"video":{}}]});
        sessions.remember_response(&source, &serde_json::to_vec(&empty).unwrap());
        assert!(sessions.observed_video(&source, 0).is_none());
        let feed = serde_json::json!({"aweme_list":[{"aweme_id":"9876543210987654321","video":{"width":1}},
            {"aweme_id":expected,"video":{}},
            {"aweme_id":expected,"desc":"当前精选视频","author":{"nickname":"作者"},"video":{"width":720,"height":1280,"play_addr":{"url_list":["https://fixture.douyinvod.com/video.mp4"]}}}]});
        sessions.remember_response(&source, &serde_json::to_vec(&feed).unwrap());
        assert_eq!(
            sessions
                .observed_video(&source, 0)
                .unwrap()
                .unwrap()
                .info
                .title,
            "当前精选视频"
        );
        let other = sites::recognize("https://www.douyin.com/video/1234567890123456790").unwrap();
        assert!(sessions.observed_video(&other, 0).is_none());
    }
    #[test]
    #[cfg(windows)]
    fn isolated_platform_sessions_are_encrypted_and_persisted() {
        let directory =
            std::env::temp_dir().join(format!("snowdown-sessions-{}", uuid::Uuid::new_v4()));
        let sessions = Sessions::open(directory.clone());
        sessions
            .save(Platform::Douyin, "sessionid=fake-douyin".into())
            .unwrap();
        sessions
            .save(Platform::Xiaohongshu, "web_session=fake-xhs".into())
            .unwrap();
        sessions
            .save_profile(
                Platform::Douyin,
                Profile {
                    name: "测试账号".into(),
                    avatar: "https://p3.douyinpic.com/avatar.jpg".into(),
                },
            )
            .unwrap();
        let bytes = std::fs::read(directory.join("douyin.dat")).unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("fake-douyin"));
        let restored = Sessions::open(directory.clone());
        assert_eq!(restored.cookie(Platform::Douyin), "sessionid=fake-douyin");
        assert_eq!(
            restored.snapshot()[0].account.as_ref().unwrap().name,
            "测试账号"
        );
        assert_eq!(
            restored.cookie(Platform::Xiaohongshu),
            "web_session=fake-xhs"
        );
        restored.clear(Platform::Douyin).unwrap();
        assert!(!directory.join("douyin.dat").exists());
        assert!(!directory.join("douyin.profile.dat").exists());
        assert!(directory.join("xiaohongshu.dat").exists());
        std::fs::remove_dir_all(directory).unwrap();
    }
}
