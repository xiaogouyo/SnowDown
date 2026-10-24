use crate::{auth, model::Platform, sites};
use serde::Serialize;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStatus {
    pub platform: Platform,
    pub saved: bool,
}

pub struct Sessions {
    directory: PathBuf,
    cookies: Mutex<HashMap<Platform, String>>,
    media: Mutex<HashMap<(Platform, String), (Instant, sites::Resolved)>>,
    observed: Mutex<Observed>,
}
#[derive(Default)]
struct Observed {
    responses: u64,
    errors: u64,
    details: HashMap<String, (Instant, serde_json::Value)>,
}

impl Sessions {
    pub fn open(directory: PathBuf) -> Self {
        let mut cookies = HashMap::new();
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
        Self {
            directory,
            cookies: Mutex::new(cookies),
            media: Mutex::new(HashMap::new()),
            observed: Mutex::new(Observed::default()),
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
        let detail = &value["aweme_detail"];
        let id = detail["aweme_id"]
            .as_str()
            .map(str::to_string)
            .or_else(|| detail["aweme_id"].as_u64().map(|n| n.to_string()));
        let Ok(expected) = sites::video_id(source) else {
            return;
        };
        if id.as_deref() != Some(&expected) {
            return;
        }
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
            "video": video
        }});
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
            })
            .collect()
    }
    pub fn save(&self, platform: Platform, cookie: String) -> Result<(), String> {
        if platform == Platform::Bilibili {
            return Err("B站请使用官方二维码登录。".into());
        }
        if cookie.is_empty() || cookie.len() > 64 * 1024 || cookie.chars().any(char::is_control) {
            return Err("未取得有效网页会话，请先在官方页面完成登录或验证。".into());
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
        if platform == Platform::Douyin {
            self.observed.lock().unwrap().details.clear();
        }
        self.media
            .lock()
            .unwrap()
            .retain(|(site, _), _| *site != platform);
        Ok(())
    }

    pub async fn resolve(
        &self,
        app: &AppHandle,
        source: &sites::Source,
        quality: u32,
    ) -> Result<sites::Resolved, String> {
        let canonical = sites::canonical_source(source, &self.cookie(source.platform)).await?;
        let source = &canonical;
        let key = (source.platform, sites::video_id(source)?);
        {
            let mut media = self.media.lock().unwrap();
            media.retain(|_, (time, _)| time.elapsed() < Duration::from_secs(300));
            if quality > 0 {
                if let Some((_, cached)) =
                    media.get(&key).filter(|(_, item)| item.quality == quality)
                {
                    return Ok(cached.clone());
                }
            }
        }
        if let Ok(result) = read_video(app, source, quality).await {
            self.media
                .lock()
                .unwrap()
                .insert(key, (Instant::now(), result.clone()));
            return Ok(result);
        }
        let result = match sites::resolve(source, quality, &self.cookie(source.platform)).await {
            Ok(resolved) => resolved,
            Err(native_error) => match read_video(app, source, quality).await {
                Ok(resolved) => resolved,
                Err(browser_error) => {
                    return Err(format!("{native_error} 官方页面：{browser_error}"))
                }
            },
        };
        let result_key = (source.platform, result.info.bvid.clone());
        let mut media = self.media.lock().unwrap();
        if media.len() >= 100 {
            media.clear();
        }
        media.insert(result_key, (Instant::now(), result.clone()));
        Ok(result)
    }
}

pub fn open_official(
    app: &AppHandle,
    platform: Platform,
    input: Option<String>,
) -> Result<(), String> {
    if platform == Platform::Bilibili {
        return Err("B站请使用官方二维码登录。".into());
    }
    let refresh_video = input.is_some();
    let url = if let Some(input) = input {
        let source = sites::recognize(&input)?;
        if source.platform != platform {
            return Err("链接与登录平台不一致。".into());
        }
        source.url.parse().map_err(|_| "视频链接无效。")?
    } else {
        platform.home().parse().unwrap()
    };
    let label = format!("official-{}", platform.key());
    if let Some(window) = app.get_webview_window(&label) {
        if refresh_video || window.url().ok().as_ref() != Some(&url) {
            window.navigate(url).map_err(|_| "无法打开视频页面。")?;
        }
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(());
    }
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|_| "应用数据目录不可用。")?
        .join("official-profiles")
        .join(platform.key());
    std::fs::create_dir_all(&directory).map_err(|_| "官方网页会话目录不可用。")?;
    let observe = platform == Platform::Douyin;
    let initial = if observe {
        "about:blank".parse().unwrap()
    } else {
        url.clone()
    };
    let window = WebviewWindowBuilder::new(app, label, WebviewUrl::External(initial))
        .title(format!(
            "{}官方页面 · 播放后关闭窗口返回 SnowDown",
            platform.label()
        ))
        .inner_size(1100.0, 780.0)
        .min_inner_size(800.0, 600.0)
        .data_directory(directory)
        .build()
        .map_err(|e| format!("无法打开官方页面：{e}"))?;
    if observe {
        crate::web_capture::attach(&window, url)?;
    }
    Ok(())
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
    if let Some(path) = &path {
        let _ = tokio::fs::write(path, b"{\"stage\":\"reading\"}").await;
    }
    let result = read_video_inner(app, source, quality).await;
    if let Some(path) = &path {
        let mut diagnostic: serde_json::Value = tokio::fs::read(path)
            .await
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_else(|| serde_json::json!({}));
        let window = app.get_webview_window(&format!("official-{}", source.platform.key()));
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
        .get_webview_window(&format!("official-{}", source.platform.key()))
        .ok_or("未打开官方窗口。")?;
    let current = sites::recognize(window.url().map_err(|_| "无法读取官方页面。")?.as_str())?;
    if current.platform != source.platform || sites::video_id(&current)? != sites::video_id(source)?
    {
        return Err("官方窗口不是当前视频。".into());
    }
    let script = include_str!("site_video.js").replace(
        "__VIDEO_ID__",
        &serde_json::to_string(&sites::video_id(source)?).unwrap(),
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
    let raw = tokio::time::timeout(Duration::from_secs(5), rx)
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
    let result = sites::from_browser(source, &data, quality);
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
            "blobPlayer": data["player"]["url"].as_str().is_some_and(|url| url.starts_with("blob:"))
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

#[cfg(debug_assertions)]
pub fn start_debug_probe(app: AppHandle) {
    let Ok(input) = std::env::var("SNOWDOWN_TEST_SOURCE") else {
        return;
    };
    let Ok(source) = sites::recognize(&input) else {
        return;
    };
    if source.platform == Platform::Bilibili {
        return;
    }
    tauri::async_runtime::spawn(async move {
        if open_official(&app, source.platform, Some(source.url.clone())).is_err() {
            return;
        }
        for _ in 0..45 {
            tokio::time::sleep(Duration::from_secs(2)).await;
            if let Ok(resolved) = read_video(&app, &source, 0).await {
                if std::env::var_os("SNOWDOWN_VERIFY_RETURN").is_some() {
                    if let Some(window) =
                        app.get_webview_window(&format!("official-{}", source.platform.key()))
                    {
                        let _ = window.close();
                    }
                    tokio::time::sleep(Duration::from_secs(8)).await;
                    if let Some(window) = app.get_webview_window("main") {
                        let (sender, receiver) = tokio::sync::oneshot::channel();
                        let sender = Mutex::new(Some(sender));
                        let title = serde_json::to_string(&resolved.info.title).unwrap();
                        let script = format!("JSON.stringify({{modalVisible:!!document.querySelector('.video-modal'),titleMatches:document.querySelector('.video-modal')?.textContent.includes({title})??false,qualityVisible:!!document.querySelector('.video-modal .ui-select'),downloadVisible:!!document.querySelector('.video-modal .modal-footer button')}})");
                        let _ = window.eval_with_callback(script, move |value| {
                            if let Some(sender) = sender.lock().unwrap().take() {
                                let _ = sender.send(value);
                            }
                        });
                        if let Ok(Ok(raw)) =
                            tokio::time::timeout(Duration::from_secs(5), receiver).await
                        {
                            let value = serde_json::from_str::<serde_json::Value>(&raw)
                                .ok()
                                .and_then(|value| value.as_str().map(str::to_owned))
                                .unwrap_or(raw);
                            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                                .join("../.tools/v012-return-verification.json");
                            let _ = tokio::fs::write(path, value).await;
                        }
                    }
                }
                if std::env::var_os("SNOWDOWN_VERIFY_MEDIA").is_some() {
                    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .join("../.tools/samples/douyin-validation.mp4");
                    let _ = tokio::fs::remove_file(&path).await;
                    let log = diagnostics_path().unwrap();
                    let _ = tokio::fs::write(
                        &log,
                        b"{\"stage\":\"downloading\",\"parseSuccess\":true}",
                    )
                    .await;
                    if let Ok(client) = crate::engine::media_client(source.platform) {
                        let cancel = std::sync::atomic::AtomicBool::new(false);
                        match crate::engine::transfer(
                            &client,
                            &resolved.url,
                            &path,
                            &cancel,
                            0,
                            |_, _, _| {},
                        )
                        .await
                        {
                            Ok(bytes) => {
                                let mut bytes = bytes;
                                let mut verified_path = path.clone();
                                if let Some(audio_url) = &resolved.audio_url {
                                    let audio_path = path.with_extension("audio.part");
                                    let _ = tokio::fs::remove_file(&audio_path).await;
                                    if let Ok(received) = crate::engine::transfer(
                                        &client,
                                        audio_url,
                                        &audio_path,
                                        &cancel,
                                        0,
                                        |_, _, _| {},
                                    )
                                    .await
                                    {
                                        bytes += received;
                                        verified_path = path.with_file_name("douyin-verified.mp4");
                                        let args = [
                                            "-nostdin",
                                            "-y",
                                            "-hide_banner",
                                            "-loglevel",
                                            "error",
                                            "-i",
                                        ]
                                        .into_iter()
                                        .map(str::to_string)
                                        .chain([
                                            path.to_string_lossy().to_string(),
                                            "-i".into(),
                                            audio_path.to_string_lossy().to_string(),
                                            "-map".into(),
                                            "0:v:0".into(),
                                            "-map".into(),
                                            "1:a:0".into(),
                                            "-c".into(),
                                            "copy".into(),
                                            "-movflags".into(),
                                            "+faststart".into(),
                                            verified_path.to_string_lossy().to_string(),
                                        ])
                                        .collect::<Vec<_>>();
                                        let _ = crate::engine::run_ffmpeg(
                                            &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                                                .join("resources/ffmpeg.exe"),
                                            &args,
                                            &cancel,
                                        )
                                        .await;
                                    }
                                }
                                let mut command = tokio::process::Command::new(
                                    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                                        .join("resources/ffprobe.exe"),
                                );
                                command
                                    .args([
                                        "-v",
                                        "error",
                                        "-show_entries",
                                        "stream=codec_type:format=duration",
                                        "-of",
                                        "json",
                                    ])
                                    .arg(&verified_path);
                                #[cfg(windows)]
                                command.creation_flags(0x08000000);
                                let output = command.output().await;
                                let metadata = output
                                    .ok()
                                    .and_then(|o| {
                                        serde_json::from_slice::<serde_json::Value>(&o.stdout).ok()
                                    })
                                    .unwrap_or_default();
                                let streams = metadata["streams"].as_array();
                                let video = streams
                                    .is_some_and(|s| s.iter().any(|s| s["codec_type"] == "video"));
                                let audio = streams
                                    .is_some_and(|s| s.iter().any(|s| s["codec_type"] == "audio"));
                                let result = serde_json::json!({"stage":"downloaded","bytes":bytes,"hasVideo":video,"hasAudio":audio,"duration":metadata["format"]["duration"],"success":video&&audio});
                                let _ =
                                    tokio::fs::write(&log, serde_json::to_vec(&result).unwrap())
                                        .await;
                            }
                            Err(_) => {
                                let _ = tokio::fs::write(
                                    &log,
                                    b"{\"stage\":\"download-failed\",\"parseSuccess\":true}",
                                )
                                .await;
                            }
                        }
                    }
                }
                break;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let bytes = std::fs::read(directory.join("douyin.dat")).unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("fake-douyin"));
        let restored = Sessions::open(directory.clone());
        assert_eq!(restored.cookie(Platform::Douyin), "sessionid=fake-douyin");
        assert_eq!(
            restored.cookie(Platform::Xiaohongshu),
            "web_session=fake-xhs"
        );
        restored.clear(Platform::Douyin).unwrap();
        assert!(!directory.join("douyin.dat").exists());
        assert!(directory.join("xiaohongshu.dat").exists());
        std::fs::remove_dir_all(directory).unwrap();
    }
}
