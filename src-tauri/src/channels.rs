//! Official Tencent-session based Channels adapter. Never reads another browser's profile.
use crate::{auth, bilibili, model::*, sites};
use futures_util::StreamExt;
use reqwest::{header, Client};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};
use url::Url;

const LOGIN: &str = "https://yuanbao.tencent.com/";
const PARSE: &str = "https://yuanbao.tencent.com/api/weixin/get_parse_result";
const FEED: &str = "https://channels.weixin.qq.com/finder-preview/api/feed/get_feed_info";
const WINDOW: &str = "official-channels-yuanbao";
const LOGIN_REQUIRED: &str =
    "视频号解析需要腾讯元宝官方会话，请在设置 → 平台账号 → 视频号完成官方登录，再保存会话并重试。";
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub saved: bool,
    pub state: String,
    pub message: String,
}
#[derive(Default)]
struct Session {
    cookie: String,
    verified: bool,
    generation: u64,
}
pub struct Channels {
    path: PathBuf,
    session: Mutex<Session>,
    cache: Mutex<HashMap<String, (Instant, sites::Resolved)>>,
    resolver: tokio::sync::Mutex<()>,
}

pub fn normalize(input: &str) -> Result<String, String> {
    let u = Url::parse(input).map_err(|_| "视频号链接无效。")?;
    if u.scheme() != "https"
        || !u.username().is_empty()
        || u.password().is_some()
        || u.port().is_some()
    {
        return Err("请使用视频号 HTTPS 分享链接。".into());
    }
    let short = match u.host_str() {
        Some("weixin.qq.com") => u
            .path()
            .strip_prefix("/sph/")
            .map(|s| s.trim_end_matches('/').to_string()),
        Some("channels.weixin.qq.com") if u.path() == "/finder-preview/pages/sph" => u
            .query_pairs()
            .find(|(k, _)| k == "id")
            .map(|(_, v)| v.into_owned()),
        _ => None,
    };
    if let Some(id) = short.filter(|s| {
        (6..=64).contains(&s.len())
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    }) {
        return Ok(format!("https://weixin.qq.com/sph/{id}"));
    }
    if u.host_str() == Some("channels.weixin.qq.com") && u.path() == "/finder-preview/pages/feed" {
        let eid = u
            .query_pairs()
            .find(|(k, _)| k == "eid")
            .map(|(_, v)| v.into_owned())
            .filter(|s| !s.is_empty() && s.len() <= 4096)
            .ok_or("视频号详情链接缺少视频标识。")?;
        let mut out =
            Url::parse("https://channels.weixin.qq.com/finder-preview/pages/feed").unwrap();
        out.query_pairs_mut().append_pair("eid", &eid);
        if let Some((_, token)) = u.query_pairs().find(|(k, _)| k == "token") {
            if token.len() > 8192 {
                return Err("视频号访问凭据过长。".into());
            }
            out.query_pairs_mut().append_pair("token", &token);
        }
        return Ok(out.to_string());
    }
    Err("请粘贴 weixin.qq.com/sph/ 开头的视频号分享链接。".into())
}
pub fn video_id(input: &str) -> Result<String, String> {
    let u = Url::parse(&normalize(input)?).unwrap();
    Ok(if u.host_str() == Some("weixin.qq.com") {
        u.path()
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap()
            .into()
    } else {
        format!(
            "{:x}",
            md5::compute(
                u.query_pairs()
                    .find(|(k, _)| k == "eid")
                    .unwrap()
                    .1
                    .as_bytes()
            )
        )
    })
}
pub fn media_url(input: &str) -> Result<String, String> {
    let u = crate::resource_download::validate(input)?;
    let host = u.host_str().unwrap_or_default();
    if ![
        "finder.video.qq.com",
        "video.weixin.qq.com",
        "finder.video.weixin.qq.com",
        "wxapp.tc.qq.com",
        "qpic.cn",
        "qlogo.cn",
    ]
    .iter()
    .any(|base| sites::host_matches(host, base))
    {
        return Err("视频号返回了非腾讯媒体地址，已停止下载。".into());
    }
    Ok(u.to_string())
}
fn client(cookie: &str) -> Result<Client, String> {
    let mut headers = header::HeaderMap::new();
    if !cookie.is_empty() {
        headers.insert(
            header::COOKIE,
            header::HeaderValue::from_str(cookie).map_err(|_| "解析会话无效，请重新登录。")?,
        );
    }
    Client::builder()
        .user_agent(bilibili::USER_AGENT)
        .default_headers(headers)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(18))
        .build()
        .map_err(|_| "视频号网络组件不可用。".into())
}
async fn read_json(response: reqwest::Response) -> Result<Value, String> {
    if response.status().as_u16() == 401 {
        return Err(LOGIN_REQUIRED.into());
    }
    if response.status().as_u16() == 403 {
        return Err("官方服务需要验证或暂时限制访问，请在腾讯元宝官方页面完成验证后重试。".into());
    }
    if !response.status().is_success() {
        return Err(format!(
            "官方解析服务暂不可用（HTTP {}），请稍后重试。",
            response.status().as_u16()
        ));
    }
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| "官方解析响应中断，请重试。")?;
        if bytes.len() + chunk.len() > 4 * 1024 * 1024 {
            return Err("官方解析响应过大。".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes)
        .map_err(|_| "官方服务暂未返回有效的视频数据，请完成登录或验证后重试。".into())
}
async fn parse_share(source: &str, cookie: &str) -> Result<Value, String> {
    let response = client(cookie)?
        .post(PARSE)
        .header(header::ORIGIN, LOGIN.trim_end_matches('/'))
        .header(header::REFERER, LOGIN)
        .json(&json!({"type":"video_channel_url","url":source,"scene":1}))
        .send()
        .await
        .map_err(|_| "无法连接腾讯元宝官方解析服务，请检查网络。")?;
    let value = read_json(response).await?;
    if !value["data"].is_object() {
        return Err("腾讯元宝尚未返回视频号详情，请在官方页面检查登录或验证状态。".into());
    }
    Ok(value["data"].clone())
}
fn feed_destination(source: &str, parsed: &Value) -> Result<(String, String), String> {
    let original = Url::parse(source).map_err(|_| "视频号链接无效。")?;
    let playable = parsed["playable_url"]
        .as_str()
        .and_then(|s| Url::parse(s).ok())
        .unwrap_or(original);
    if playable.scheme() != "https"
        || playable.host_str() != Some("channels.weixin.qq.com")
        || !playable.username().is_empty()
        || playable.password().is_some()
        || playable.port().is_some()
    {
        return Err("官方解析未返回有效的视频号播放页面。".into());
    }
    let eid = playable
        .query_pairs()
        .find(|(k, _)| k == "eid")
        .map(|(_, v)| v.into_owned())
        .or_else(|| parsed["wx_export_id"].as_str().map(str::to_owned))
        .filter(|s| !s.is_empty() && s.len() <= 4096)
        .ok_or("官方解析未返回视频标识。")?;
    let token = playable
        .query_pairs()
        .find(|(k, _)| k == "token")
        .map(|(_, v)| v.into_owned())
        .unwrap_or_default();
    if token.len() > 8192 {
        return Err("视频号访问凭据过长。".into());
    }
    Ok((eid, token))
}
fn resolved(source: &str, value: &Value) -> Result<sites::Resolved, String> {
    if value["errCode"].as_i64().unwrap_or(0) != 0
        || value["data"]["errMsg"]["type"].as_i64().unwrap_or(0) != 0
    {
        return Err("视频号内容不可访问、已失效或需要重新验证，请在官方页面确认。".into());
    }
    let feed = &value["data"]["feedInfo"];
    if feed["mediaType"].as_u64().is_some_and(|n| n != 4) {
        return Err("当前链接不是普通视频，首版暂不支持图集和直播。".into());
    }
    let raw = feed["h264VideoInfo"]["videoUrl"]
        .as_str()
        .filter(|s| !s.is_empty())
        .or_else(|| feed["videoUrl"].as_str().filter(|s| !s.is_empty()))
        .ok_or("官方响应只有预览信息，尚未取得可下载视频；请检查解析会话或换一个有效视频链接。")?;
    if ["decodeKey", "decode_key"].iter().any(|key| {
        feed[*key]
            .as_str()
            .is_some_and(|s| !s.is_empty() && s != "0")
            || feed[*key].as_u64().is_some_and(|n| n != 0)
    }) {
        return Err("当前返回的是需要额外处理的媒体，暂不将其作为可播放视频保存。".into());
    }
    let url = media_url(raw)?;
    let id = video_id(source)?;
    let width = feed["width"]
        .as_u64()
        .or(feed["videoWidth"].as_u64())
        .unwrap_or(0);
    let height = feed["height"]
        .as_u64()
        .or(feed["videoHeight"].as_u64())
        .unwrap_or(0);
    let quality = width.min(height).min(u32::MAX as u64) as u32;
    let title = feed["description"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.chars().take(200).collect::<String>())
        .unwrap_or_else(|| format!("视频号_{id}"));
    let cover = feed["coverUrl"]
        .as_str()
        .and_then(|s| media_url(s).ok())
        .unwrap_or_default();
    let duration = feed["duration"].as_u64().unwrap_or(0);
    Ok(sites::Resolved {
        url,
        quality,
        audio_url: None,
        info: VideoInfo {
            platform: Platform::Channels,
            source_url: source.into(),
            bvid: id,
            title: title.clone(),
            cover,
            author: value["data"]["authorInfo"]["nickname"]
                .as_str()
                .unwrap_or_default()
                .chars()
                .take(80)
                .collect(),
            duration,
            pages: vec![VideoPage {
                cid: 1,
                page: 1,
                part: title,
                duration,
            }],
            qualities: vec![Quality {
                id: quality,
                label: if quality == 0 {
                    "官方可用画质".into()
                } else {
                    format!("{quality}P · 官方可用画质")
                },
                width,
                height,
            }],
            selected_cid: 1,
        },
    })
}
impl Channels {
    pub fn open(path: PathBuf) -> Self {
        let cookie = std::fs::read(&path)
            .ok()
            .and_then(|b| auth::crypt(&b, false).ok())
            .and_then(|b| String::from_utf8(b).ok())
            .unwrap_or_default();
        Self {
            path,
            session: Mutex::new(Session {
                cookie,
                ..Default::default()
            }),
            cache: Mutex::new(HashMap::new()),
            resolver: tokio::sync::Mutex::new(()),
        }
    }
    pub fn status(&self) -> Status {
        let s = self.session.lock().unwrap();
        Status {
            saved: !s.cookie.is_empty(),
            state: if s.verified {
                "authenticated"
            } else if s.cookie.is_empty() {
                "loggedout"
            } else {
                "unknown"
            }
            .into(),
            message: if s.verified {
                "官方解析会话已通过检查。"
            } else if s.cookie.is_empty() {
                "请打开腾讯元宝官方页面完成登录。"
            } else {
                "已加密保存解析会话，解析视频时会验证其有效性。"
            }
            .into(),
        }
    }
    pub async fn resolve(
        &self,
        source: &sites::Source,
        quality: u32,
    ) -> Result<sites::Resolved, String> {
        let canonical = normalize(&source.url)?;
        let _guard = self.resolver.lock().await;
        if let Some((at, v)) = self.cache.lock().unwrap().get(&canonical) {
            if at.elapsed() < Duration::from_secs(45) && (quality == 0 || quality == v.quality) {
                return Ok(v.clone());
            }
        }
        let (cookie, generation) = {
            let s = self.session.lock().unwrap();
            (s.cookie.clone(), s.generation)
        };
        let parsed = if canonical.starts_with("https://weixin.qq.com/") {
            if cookie.is_empty() {
                return Err(LOGIN_REQUIRED.into());
            }
            parse_share(&canonical, &cookie).await?
        } else {
            Value::Null
        };
        let (eid, token) = feed_destination(&canonical, &parsed)?;
        let r = client("")?
            .post(FEED)
            .header(header::ORIGIN, "https://channels.weixin.qq.com")
            .header(header::REFERER, "https://channels.weixin.qq.com/")
            .json(&json!({"baseReq":{"generalToken":token},"exportId":eid}))
            .send()
            .await
            .map_err(|_| "无法连接视频号官方媒体接口。")?;
        let feed = read_json(r).await?;
        #[cfg(debug_assertions)]
        if let Some(path) = std::env::var_os("SNOWDOWN_CHANNELS_DIAGNOSTICS") {
            fn schema(v: &Value, depth: usize) -> Value {
                if depth > 6 {
                    return json!("nested");
                };
                match v {
                    Value::Object(m) => m
                        .iter()
                        .take(80)
                        .map(|(k, v)| (k.clone(), schema(v, depth + 1)))
                        .collect::<serde_json::Map<_, _>>()
                        .into(),
                    Value::Array(a) => {
                        json!({"count":a.len(),"sample":a.iter().take(2).map(|v|schema(v,depth+1)).collect::<Vec<_>>()})
                    }
                    Value::String(s) => json!({"stringLength":s.len()}),
                    _ => v.clone(),
                }
            }
            let diagnostic = json!({"share":schema(&parsed,0),"feed":schema(&feed,0)});
            let _ = std::fs::write(
                path,
                serde_json::to_vec_pretty(&diagnostic).unwrap_or_default(),
            );
        }
        let result = resolved(&canonical, &feed)?;
        let mut s = self.session.lock().unwrap();
        if s.generation != generation {
            return Err("解析会话已改变，请重新解析。".into());
        }
        s.verified = !s.cookie.is_empty();
        if quality != 0 && quality != result.quality {
            return Err("原画质已不可用，请重新解析并选择当前画质。".into());
        }
        let mut cache = self.cache.lock().unwrap();
        if cache.len() >= 32 {
            cache.clear()
        }
        cache.insert(canonical, (Instant::now(), result.clone()));
        Ok(result)
    }
    pub async fn save(&self, app: &AppHandle, input: Option<&str>) -> Result<Status, String> {
        let generation = self.session.lock().unwrap().generation;
        let window = app
            .get_webview_window(WINDOW)
            .ok_or("请先打开腾讯元宝官方登录页面。")?;
        let cookie = tokio::time::timeout(
            Duration::from_secs(5),
            tauri::async_runtime::spawn_blocking(move || {
                window.cookies_for_url(Url::parse(LOGIN).unwrap())
            }),
        )
        .await
        .map_err(|_| "读取官方会话超时。")?
        .map_err(|_| "读取官方会话失败。")?
        .map_err(|_| "读取官方会话失败。")?
        .into_iter()
        .map(|c| format!("{}={}", c.name(), c.value()))
        .collect::<Vec<_>>()
        .join("; ");
        if cookie.is_empty() || cookie.len() > 32768 {
            return Err("尚未取得有效会话，请在官方页面完成登录。".into());
        }
        if let Some(input) = input.filter(|s| !s.trim().is_empty()) {
            let canonical = normalize(input)?;
            parse_share(&canonical, &cookie).await?;
        }
        let mut s = self.session.lock().unwrap();
        if s.generation != generation {
            return Err("会话状态已改变，请重新保存。".into());
        }
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|_| "会话目录不可用。")?
        }
        let pending = self.path.with_extension("pending");
        std::fs::write(&pending, auth::crypt(cookie.as_bytes(), true)?)
            .map_err(|_| "加密会话保存失败。")?;
        if std::fs::rename(&pending, &self.path).is_err() {
            let _ = std::fs::remove_file(&pending);
            return Err("加密会话保存失败，原有会话已保留。".into());
        }
        s.cookie = cookie;
        s.verified = false;
        s.generation += 1;
        self.cache.lock().unwrap().clear();
        drop(s);
        Ok(self.status())
    }
    pub async fn check(&self, source: Option<&str>) -> Result<Status, String> {
        self.cache.lock().unwrap().clear();
        if let Some(input) = source.filter(|s| !s.trim().is_empty()) {
            let url = normalize(input)?;
            let source = sites::Source {
                platform: Platform::Channels,
                url,
            };
            match self.resolve(&source, 0).await {
                Ok(_) => return Ok(self.status()),
                Err(error) => {
                    self.session.lock().unwrap().verified = false;
                    return Ok(Status {
                        saved: self.status().saved,
                        state: if error == LOGIN_REQUIRED {
                            "loggedout"
                        } else {
                            "unknown"
                        }
                        .into(),
                        message: error,
                    });
                }
            }
        }
        Ok(self.status())
    }
    pub async fn logout(&self, app: &AppHandle) -> Result<Status, String> {
        {
            let mut s = self.session.lock().unwrap();
            s.generation += 1;
            s.verified = false;
            s.cookie.clear();
            self.cache.lock().unwrap().clear();
            match std::fs::remove_file(&self.path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err("清除本机解析会话失败。".into()),
            }
        }
        let window = open_login(app, false)?;
        crate::web_capture::clear_profile(&window).await?;
        let _ = window.destroy();
        Ok(self.status())
    }
}
pub fn open_login(app: &AppHandle, show: bool) -> Result<tauri::WebviewWindow, String> {
    if let Some(w) = app.get_webview_window(WINDOW) {
        if show {
            let _ = w.navigate(Url::parse(LOGIN).unwrap());
            let _ = w.show();
            let _ = w.set_focus();
        }
        return Ok(w);
    }
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|_| "平台会话目录不可用。")?
        .join("official-profiles/channels-yuanbao");
    std::fs::create_dir_all(&directory).map_err(|_| "无法创建官方会话目录。")?;
    let w = WebviewWindowBuilder::new(
        app,
        WINDOW,
        WebviewUrl::External(Url::parse(if show { LOGIN } else { "about:blank" }).unwrap()),
    )
    .title("SnowDown · 腾讯元宝官方登录（视频号解析）")
    .inner_size(1100.0, 780.0)
    .min_inner_size(800.0, 600.0)
    .visible(show)
    .focused(show)
    .data_directory(directory)
    .on_navigation(|u| {
        u.as_str() == "about:blank"
            || (u.scheme() == "https"
                && u.host_str().is_some_and(|h| {
                    ["tencent.com", "qq.com"]
                        .iter()
                        .any(|b| sites::host_matches(h, b))
                }))
    })
    .build()
    .map_err(|_| "无法打开腾讯元宝官方页面。")?;
    let keep = w.clone();
    w.on_window_event(move |event| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            let _ = keep.hide();
        }
    });
    Ok(w)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_only_supported_share_routes() {
        assert_eq!(
            normalize("https://weixin.qq.com/sph/ABC12345?from=share#x").unwrap(),
            "https://weixin.qq.com/sph/ABC12345"
        );
        assert_eq!(
            normalize("https://channels.weixin.qq.com/finder-preview/pages/sph?id=ABC12345")
                .unwrap(),
            "https://weixin.qq.com/sph/ABC12345"
        );
        for url in [
            "https://weixin.qq.com.evil.test/sph/ABC12345",
            "https://evil.weixin.qq.com/sph/ABC12345",
            "http://weixin.qq.com/sph/ABC12345",
            "https://u@weixin.qq.com/sph/ABC12345",
            "https://weixin.qq.com/sph/abc/def",
            "https://weixin.qq.com/other",
        ] {
            assert!(normalize(url).is_err())
        }
    }
    #[test]
    fn rejects_preview_and_untrusted_or_encrypted_resources() {
        let source = "https://weixin.qq.com/sph/ABC12345";
        let mut v = json!({"errCode":0,"data":{"feedInfo":{"description":"标题","mediaType":4},"authorInfo":{"nickname":"作者"}}});
        assert!(resolved(source, &v).is_err());
        v["data"]["feedInfo"]["videoUrl"] = json!("https://finder.video.qq.com/video.mp4");
        assert_eq!(resolved(source, &v).unwrap().info.author, "作者");
        v["data"]["feedInfo"]["decodeKey"] = json!("123");
        assert!(resolved(source, &v).is_err());
        v["data"]["feedInfo"]["decodeKey"] = Value::Null;
        v["data"]["feedInfo"]["videoUrl"] =
            json!("https://finder.video.qq.com.evil.test/video.mp4");
        assert!(resolved(source, &v).is_err());
        assert!(media_url("https://127.0.0.1/a.mp4").is_err())
    }
    #[test]
    fn prefers_h264_and_session_status_never_exposes_credentials() {
        let source = "https://weixin.qq.com/sph/ABC12345";
        let data = json!({"data":{"feedInfo":{"mediaType":4,"videoUrl":"https://finder.video.qq.com/other.mp4","h264VideoInfo":{"videoUrl":"https://finder.video.qq.com/avc.mp4"}}}});
        assert!(resolved(source, &data).unwrap().url.ends_with("/avc.mp4"));
        let dir = std::env::temp_dir().join(format!("snowdown-channels-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("session.dat");
        std::fs::write(&path, auth::crypt(b"session=fixture-secret", true).unwrap()).unwrap();
        let session = Channels::open(path);
        assert!(session.status().saved);
        assert_eq!(session.status().state, "unknown");
        assert!(!serde_json::to_string(&session.status())
            .unwrap()
            .contains("fixture-secret"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_wrong_playback_origin() {
        assert!(feed_destination(
            "https://weixin.qq.com/sph/ABC12345",
            &json!({"playable_url":"https://evil.test/?eid=fixture"})
        )
        .is_err());
        let v = json!({"playable_url":"https://channels.weixin.qq.com/finder-preview/pages/feed?eid=fixture&token=secret"});
        assert_eq!(
            feed_destination("https://weixin.qq.com/sph/ABC12345", &v).unwrap(),
            ("fixture".into(), "secret".into())
        );
    }
}
