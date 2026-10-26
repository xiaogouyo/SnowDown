use crate::{bilibili, model::*};
use regex::Regex;
use reqwest::{header, Client};
use serde::Serialize;
use serde_json::Value;
use std::{collections::BTreeMap, sync::OnceLock, time::Duration};
use url::Url;

#[derive(Clone, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub platform: Platform,
    pub url: String,
}

#[derive(Clone)]
struct Stream {
    url: String,
    width: u64,
    height: u64,
    quality: u32,
}
#[derive(Clone)]
pub struct Resolved {
    pub info: VideoInfo,
    pub url: String,
    pub quality: u32,
    pub audio_url: Option<String>,
}

pub fn host_matches(host: &str, base: &str) -> bool {
    host == base || host.ends_with(&format!(".{base}"))
}

#[derive(serde::Deserialize)]
pub struct CatalogSite {
    pub id: String,
    pub label: String,
    pub hosts: Vec<String>,
}
pub fn catalog() -> &'static Vec<CatalogSite> {
    static CATALOG: OnceLock<Vec<CatalogSite>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!("../../shared/platforms.json"))
            .expect("bundled site catalog")
    })
}
pub fn catalog_site(host: &str) -> Option<&'static CatalogSite> {
    catalog()
        .iter()
        .find(|site| site.hosts.iter().any(|base| host_matches(host, base)))
}

pub fn source_host(host: &str) -> Option<Platform> {
    if host_matches(host, "bilibili.com") || host == "b23.tv" {
        Some(Platform::Bilibili)
    } else if host_matches(host, "douyin.com") || host_matches(host, "iesdouyin.com") {
        Some(Platform::Douyin)
    } else if host_matches(host, "xiaohongshu.com")
        || host == "xhslink.com"
        || host.ends_with(".xhslink.com")
    {
        Some(Platform::Xiaohongshu)
    } else if catalog_site(host).is_some() {
        Some(Platform::Web)
    } else {
        None
    }
}

pub fn recognize(input: &str) -> Result<Source, String> {
    let text = input.trim();
    if (text.starts_with("BV") || text.to_ascii_lowercase().starts_with("av"))
        && !text.contains('/')
    {
        let (id, _) = bilibili::normalize_input(text)?;
        return Ok(Source {
            platform: Platform::Bilibili,
            url: format!("https://www.bilibili.com/video/{id}"),
        });
    }
    static URL_PATTERN: OnceLock<Regex> = OnceLock::new();
    let regex = URL_PATTERN
        .get_or_init(|| Regex::new(r#"https?://[^\s<>\"'“”，。！？（）【】()\[\]{}]+"#).unwrap());
    for candidate in regex.find_iter(text) {
        let Ok(mut url) = Url::parse(candidate.as_str().trim_end_matches(['.', ',', ';'])) else {
            continue;
        };
        if !url.username().is_empty() || url.password().is_some() || url.port().is_some() {
            continue;
        }
        let Some(platform) = source_host(url.host_str().unwrap_or_default()) else {
            continue;
        };
        url.set_fragment(None);
        let _ = url.set_scheme("https");
        if platform == Platform::Bilibili {
            let (id, page) = bilibili::normalize_input(url.as_str())?;
            if id.starts_with("https://") {
                return Ok(Source { platform, url: id });
            }
            let suffix = if page > 1 {
                format!("?p={page}")
            } else {
                String::new()
            };
            return Ok(Source {
                platform,
                url: format!("https://www.bilibili.com/video/{id}{suffix}"),
            });
        }
        if platform == Platform::Douyin && url.host_str() != Some("v.douyin.com") {
            let id = url
                .path_segments()
                .and_then(|s| {
                    let parts: Vec<_> = s.collect();
                    parts.windows(2).find(|p| p[0] == "video").map(|p| p[1])
                })
                .map(str::to_owned)
                .or_else(|| {
                    url.query_pairs()
                        .find(|(k, _)| matches!(k.as_ref(), "modal_id" | "aweme_id" | "vid"))
                        .map(|(_, v)| v.to_string())
                });
            if let Some(id) = id
                .filter(|s| s.len() >= 8 && s.len() <= 24 && s.chars().all(|c| c.is_ascii_digit()))
            {
                return Ok(Source {
                    platform,
                    url: format!("https://www.douyin.com/video/{id}"),
                });
            }
            return Err("请使用抖音视频页面或分享短链接。".into());
        }
        if platform == Platform::Xiaohongshu
            && host_matches(url.host_str().unwrap_or_default(), "xiaohongshu.com")
        {
            let id = url
                .path_segments()
                .and_then(|s| {
                    let parts: Vec<_> = s.collect();
                    parts
                        .windows(2)
                        .find(|p| p[0] == "explore" || p[0] == "item")
                        .map(|p| p[1])
                })
                .filter(|id| id.len() == 24 && id.chars().all(|c| c.is_ascii_hexdigit()))
                .map(str::to_owned)
                .ok_or("请使用小红书视频笔记或分享短链接。")?;
            let query: Vec<_> = url
                .query_pairs()
                .filter(|(k, _)| matches!(k.as_ref(), "xsec_token" | "xsec_source"))
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect();
            let mut canonical =
                Url::parse(&format!("https://www.xiaohongshu.com/explore/{id}")).unwrap();
            if !query.is_empty() {
                canonical.query_pairs_mut().extend_pairs(query);
            }
            return Ok(Source {
                platform,
                url: canonical.to_string(),
            });
        }
        return Ok(Source {
            platform,
            url: {
                if platform == Platform::Web {
                    crate::resource_download::validate(url.as_str())?;
                }
                url.to_string()
            },
        });
    }
    Err("请粘贴B站、抖音或小红书的视频链接，也支持带文字的分享内容。".into())
}

fn client(platform: Platform, cookie: &str) -> Result<Client, String> {
    let mut headers = header::HeaderMap::new();
    headers.insert(
        header::REFERER,
        header::HeaderValue::from_static(platform.home()),
    );
    if !cookie.is_empty() {
        headers.insert(
            header::COOKIE,
            header::HeaderValue::from_str(cookie).map_err(|_| "平台会话无效，请重新登录。")?,
        );
    }
    Client::builder()
        .user_agent(bilibili::USER_AGENT)
        .default_headers(headers)
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "网络组件初始化失败。".into())
}

async fn page(client: &Client, source: &Source) -> Result<(String, Url), String> {
    let mut current = Url::parse(&source.url).map_err(|_| "视频链接无效。")?;
    for _ in 0..6 {
        if current.scheme() != "https"
            || source_host(current.host_str().unwrap_or_default()) != Some(source.platform)
        {
            return Err("分享链接跳转到了非所属平台，已停止解析。".into());
        }
        let response = client
            .get(current.clone())
            .send()
            .await
            .map_err(|_| format!("无法连接{}，请检查网络后重试。", source.platform.label()))?;
        if response.status().is_redirection() {
            let next = response
                .headers()
                .get(header::LOCATION)
                .and_then(|h| h.to_str().ok())
                .ok_or("分享链接跳转信息缺失。")?;
            current = current.join(next).map_err(|_| "分享链接跳转无效。")?;
            continue;
        }
        if !response.status().is_success() {
            return Err(format!(
                "{}暂时无法访问（HTTP {}）。可在设置中打开官方页面登录后重试。",
                source.platform.label(),
                response.status().as_u16()
            ));
        }
        let bytes = response.bytes().await.map_err(|_| "页面读取失败。")?;
        if bytes.len() > 8 * 1024 * 1024 {
            return Err("页面数据过大，无法解析。".into());
        }
        return Ok((String::from_utf8_lossy(&bytes).into(), current));
    }
    Err("分享链接跳转次数过多。".into())
}

pub async fn canonical_source(source: &Source, cookie: &str) -> Result<Source, String> {
    if !source.url.contains("v.douyin.com/") && !source.url.contains("xhslink.com/") {
        return Ok(source.clone());
    }
    let http = client(source.platform, cookie)?;
    let (_, final_url) = page(&http, source).await?;
    if final_url.path().contains("login") {
        return Err("平台要求登录，请先在设置 → 平台账号中打开官方页面并保存会话。".into());
    }
    let resolved = recognize(final_url.as_str())?;
    if resolved.url.contains("v.douyin.com/") || resolved.url.contains("xhslink.com/") {
        return Err("分享短链接未跳转到视频页面。".into());
    }
    Ok(resolved)
}

pub async fn resolve(source: &Source, quality: u32, cookie: &str) -> Result<Resolved, String> {
    let http = client(source.platform, cookie)?;
    let canonical = canonical_source(source, cookie).await?;
    let id = Url::parse(&canonical.url)
        .ok()
        .and_then(|u| {
            u.path_segments()
                .and_then(|s| s.filter(|s| !s.is_empty()).next_back().map(str::to_owned))
        })
        .ok_or("没有找到视频标识。")?;
    let mut audio_url = None;
    let metadata = match source.platform {
        Platform::Douyin => {
            let response = http
                .get("https://www.douyin.com/aweme/v1/web/aweme/detail/")
                .query(&[("aweme_id", &id)])
                .send()
                .await
                .map_err(|_| "抖音连接失败，请检查网络。")?;
            let data: Value = response.json().await.map_err(|_| "抖音需要有效的官方网页会话，请在设置 → 平台账号中打开抖音，完成登录或验证后导入会话。")?;
            audio_url = douyin_audio(&data["aweme_detail"]["video"]);
            parse_douyin(&data, &id)?
        }
        Platform::Xiaohongshu => {
            let (html, final_url) = page(&http, &canonical).await?;
            if final_url.path().contains("login") {
                return Err("小红书要求登录，请在设置 → 平台账号中打开小红书官方页面，登录后导入会话再解析。".into());
            }
            let state = extract_state(&html, "window.__INITIAL_STATE__")?;
            parse_xhs(&state, &id)?
        }
        Platform::Bilibili | Platform::Web => return Err("此平台使用专用解析器。".into()),
    };
    let mut resolved = build_resolved(&canonical, &id, metadata, quality)?;
    resolved.audio_url = audio_url;
    Ok(resolved)
}

fn build_resolved(
    source: &Source,
    id: &str,
    metadata: Metadata,
    quality: u32,
) -> Result<Resolved, String> {
    let (title, cover, author, duration, streams) = metadata;
    let mut qualities = BTreeMap::new();
    for stream in &streams {
        qualities.entry(stream.quality).or_insert(Quality {
            id: stream.quality,
            label: if stream.height > 0 && stream.width > 0 {
                format!("{}P", stream.quality)
            } else {
                "原始画质".into()
            },
            width: stream.width,
            height: stream.height,
        });
    }
    let selected = streams
        .iter()
        .filter(|s| s.quality <= quality || quality == 0)
        .max_by_key(|s| s.quality)
        .or_else(|| streams.iter().max_by_key(|s| s.quality))
        .ok_or("没有找到可下载的视频流；图文笔记和图集暂不支持。")?;
    let url = selected.url.clone();
    let actual = selected.quality;
    let info = VideoInfo {
        platform: source.platform,
        source_url: source.url.clone(),
        bvid: id.to_string(),
        title: title.clone(),
        cover,
        author,
        duration,
        pages: vec![VideoPage {
            cid: 1,
            page: 1,
            part: title,
            duration,
        }],
        qualities: qualities.into_values().rev().collect(),
        selected_cid: 1,
    };
    Ok(Resolved {
        info,
        url,
        quality: actual,
        audio_url: None,
    })
}

pub fn video_id(source: &Source) -> Result<String, String> {
    if source.platform == Platform::Web {
        use sha2::Digest;
        return Ok(format!("{:x}", sha2::Sha256::digest(source.url.as_bytes()))[..24].to_string());
    }
    Url::parse(&source.url)
        .ok()
        .and_then(|u| {
            u.path_segments()
                .and_then(|s| s.filter(|s| !s.is_empty()).next_back().map(str::to_owned))
        })
        .ok_or("没有找到视频标识。".into())
}

// The official page is an optional fallback when the platform requires browser
// context. Accept only the exact requested video and its playable media URL.
pub fn from_browser(source: &Source, value: &Value, quality: u32) -> Result<Resolved, String> {
    let page_source = recognize(value["pageUrl"].as_str().unwrap_or_default())?;
    let id = video_id(source)?;
    if page_source.platform != source.platform || video_id(&page_source)? != id {
        return Err("后台页面尚未加载输入链接对应的视频。".into());
    }
    if source.platform == Platform::Xiaohongshu && value["state"].is_object() {
        return build_resolved(source, &id, parse_xhs(&value["state"], &id)?, quality);
    }
    if source.platform == Platform::Douyin && value["detail"].is_object() {
        return from_detail(
            source,
            &serde_json::json!({"aweme_detail":value["detail"]}),
            quality,
        );
    }
    let player = &value["player"];
    let url = text(&player["url"]);
    if url.starts_with("blob:") || url.is_empty() {
        return Err("正在等待目标视频的详情数据。".into());
    }
    media_url(source.platform, &url)?;
    if url.to_lowercase().contains(".m3u8") || url.to_lowercase().contains(".mpd") {
        return Err("目标视频未返回可用的媒体详情。".into());
    }
    let width = player["width"].as_u64().unwrap_or(0);
    let height = player["height"].as_u64().unwrap_or(0);
    if width == 0 || height == 0 {
        return Err("目标视频尚未加载完成。".into());
    }
    let mut resolved = build_resolved(
        source,
        &id,
        (
            text(&value["title"]),
            text(&player["cover"]),
            String::new(),
            player["duration"]
                .as_f64()
                .filter(|d| d.is_finite() && *d >= 0.0)
                .unwrap_or(0.0) as u64,
            vec![Stream {
                url: url.replacen("http://", "https://", 1),
                width,
                height,
                quality: width.min(height).min(u32::MAX as u64) as u32,
            }],
        ),
        quality,
    )?;
    for q in &mut resolved.info.qualities {
        q.label = format!("{}P · 网页当前画质", q.id);
    }
    Ok(resolved)
}

type Metadata = (String, String, String, u64, Vec<Stream>);
pub fn from_detail(source: &Source, value: &Value, quality: u32) -> Result<Resolved, String> {
    let id = video_id(source)?;
    if source.platform == Platform::Xiaohongshu {
        return build_resolved(source, &id, parse_xhs(value, &id)?, quality);
    }
    let mut resolved = build_resolved(source, &id, parse_douyin(value, &id)?, quality)?;
    resolved.audio_url = douyin_audio(&value["aweme_detail"]["video"]);
    Ok(resolved)
}

fn douyin_audio(video: &Value) -> Option<String> {
    video["bit_rate_audio"]
        .as_array()?
        .iter()
        .filter_map(|entry| {
            let meta = &entry["audio_meta"];
            let urls = &meta["url_list"];
            let url = ["main_url", "backup_url", "fallback_url"]
                .into_iter()
                .filter_map(|key| urls[key].as_str())
                .find(|url| media_url(Platform::Douyin, url).is_ok())?;
            Some((
                meta["bitrate"].as_u64().unwrap_or(0),
                url.replacen("http://", "https://", 1),
            ))
        })
        .max_by_key(|(bitrate, _)| *bitrate)
        .map(|(_, url)| url)
}
fn text(value: &Value) -> String {
    value.as_str().unwrap_or_default().to_string()
}

fn parse_douyin(value: &Value, id: &str) -> Result<Metadata, String> {
    let detail = &value["aweme_detail"];
    if detail["aweme_id"].as_str() != Some(id) {
        return Err("抖音未返回该视频。请更新官方网页会话；视频也可能已删除或限制访问。".into());
    }
    let video = &detail["video"];
    let mut streams = vec![];
    if let Some(items) = video["bit_rate"].as_array() {
        for item in items {
            add_douyin_stream(&mut streams, &item["play_addr"], video);
        }
    }
    for field in ["play_addr", "play_addr_h264", "play_addr_bytevc1"] {
        add_douyin_stream(&mut streams, &video[field], video);
    }
    let cover = video["cover"]["url_list"]
        .as_array()
        .and_then(|a| a.first())
        .map(text)
        .unwrap_or_default();
    Ok((
        text(&detail["desc"]),
        cover,
        text(&detail["author"]["nickname"]),
        video["duration"].as_u64().unwrap_or(0) / 1000,
        streams,
    ))
}

fn add_douyin_stream(streams: &mut Vec<Stream>, address: &Value, video: &Value) {
    let width = address["width"]
        .as_u64()
        .or(video["width"].as_u64())
        .unwrap_or(0);
    let height = address["height"]
        .as_u64()
        .or(video["height"].as_u64())
        .unwrap_or(0);
    if let Some(url) = address["url_list"].as_array().and_then(|a| {
        a.iter().find_map(|v| {
            v.as_str()
                .filter(|s| media_url(Platform::Douyin, s).is_ok())
        })
    }) {
        streams.push(Stream {
            url: url.replace("http://", "https://"),
            width,
            height,
            quality: height.min(width.max(1)).max(1).min(u32::MAX as u64) as u32,
        });
    }
}

fn parse_xhs(value: &Value, id: &str) -> Result<Metadata, String> {
    let note = &value["note"]["noteDetailMap"][id]["note"];
    if !note.is_object() {
        return Err("小红书未返回该笔记，请确认视频可访问，并更新官方网页会话。".into());
    }
    let mut streams = vec![];
    let media = &note["video"]["media"]["stream"];
    let groups = if media["h264"].as_array().is_some_and(|a| !a.is_empty()) {
        vec![&media["h264"]]
    } else {
        media
            .as_object()
            .map(|m| m.values().collect())
            .unwrap_or_default()
    };
    for group in groups {
        for stream in group.as_array().into_iter().flatten() {
            let url = std::iter::once(&stream["masterUrl"])
                .chain(stream["backupUrls"].as_array().into_iter().flatten())
                .filter_map(Value::as_str)
                .find(|url| media_url(Platform::Xiaohongshu, url).is_ok())
                .unwrap_or_default()
                .to_string();
            if media_url(Platform::Xiaohongshu, &url).is_err() {
                continue;
            }
            let width = stream["width"].as_u64().unwrap_or(0);
            let height = stream["height"].as_u64().unwrap_or(0);
            streams.push(Stream {
                url: url.replace("http://", "https://"),
                width,
                height,
                quality: height.min(width.max(1)).max(1).min(u32::MAX as u64) as u32,
            });
        }
    }
    let cover = note["imageList"]
        .as_array()
        .and_then(|a| a.first())
        .map(|v| text(&v["urlDefault"]))
        .unwrap_or_default()
        .replace("http://", "https://");
    let duration = media
        .as_object()
        .into_iter()
        .flat_map(|groups| groups.values())
        .find_map(|group| group.as_array().and_then(|a| a.first()))
        .and_then(|v| v["duration"].as_f64())
        .map(|n| (n / 1000.0) as u64)
        .unwrap_or(0);
    Ok((
        text(&note["title"]),
        cover,
        text(&note["user"]["nickname"]),
        duration,
        streams,
    ))
}

pub fn media_url(platform: Platform, input: &str) -> Result<Url, String> {
    let url = Url::parse(input).map_err(|_| "媒体地址无效。")?;
    let host = url.host_str().ok_or("媒体地址缺少域名。")?;
    let allowed = match platform {
        Platform::Douyin => {
            (host_matches(host, "douyin.com") && url.path().starts_with("/aweme/v1/play/"))
                || [
                    "douyinvod.com",
                    "douyincdn.com",
                    "snssdk.com",
                    "byteimg.com",
                    "amemv.com",
                    "bytecdn.cn",
                ]
                .iter()
                .any(|base| host_matches(host, base))
        }
        Platform::Xiaohongshu => ["xhscdn.com", "xhscdn.net", "xiaohongshu.com"]
            .iter()
            .any(|base| host_matches(host, base)),
        _ => false,
    };
    if !allowed
        || !matches!(url.scheme(), "http" | "https")
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("平台返回了不受支持的媒体地址。".into());
    }
    Ok(url)
}

// Parse an inline JSON object without executing JavaScript. Replace bare undefined
// values only; quoted strings and escaped braces remain untouched.
fn extract_state(html: &str, marker: &str) -> Result<Value, String> {
    let start = html
        .find(marker)
        .ok_or("页面缺少视频数据，请在官方页面确认可访问后重试。")?
        + marker.len();
    let begin = html[start..]
        .find('{')
        .map(|i| i + start)
        .ok_or("页面视频数据无效。")?;
    let bytes = html.as_bytes();
    let (mut depth, mut quoted, mut escaped, mut end) = (0i32, false, false, None);
    for (i, &b) in bytes.iter().enumerate().skip(begin) {
        if quoted {
            if escaped {
                escaped = false
            } else if b == b'\\' {
                escaped = true
            } else if b == b'"' {
                quoted = false
            }
        } else if b == b'"' {
            quoted = true
        } else if b == b'{' {
            depth += 1
        } else if b == b'}' {
            depth -= 1;
            if depth == 0 {
                end = Some(i + 1);
                break;
            }
        }
    }
    let raw = &html[begin..end.ok_or("页面视频数据不完整。")?];
    let (mut result, mut quoted, mut escaped, mut index) = (String::new(), false, false, 0);
    while index < raw.len() {
        let ch = raw[index..].chars().next().unwrap();
        if !quoted && raw[index..].starts_with("undefined") {
            result.push_str("null");
            index += 9;
            continue;
        }
        result.push(ch);
        index += ch.len_utf8();
        if quoted {
            if escaped {
                escaped = false
            } else if ch == '\\' {
                escaped = true
            } else if ch == '"' {
                quoted = false
            }
        } else if ch == '"' {
            quoted = true
        }
    }
    serde_json::from_str(&result)
        .map_err(|_| "页面视频数据格式已变化，请更新软件或稍后重试。".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn douyin_metadata_uses_correct_video_and_portrait_qualities() {
        let source = recognize("https://www.douyin.com/video/1234567890123456789").unwrap();
        let data = serde_json::json!({"aweme_detail":{"aweme_id":"1234567890123456789","desc":"测试视频","author":{"nickname":"作者"},"video":{"duration":12500,"width":1080,"height":1920,"bit_rate":[{"play_addr":{"width":720,"height":1280,"url_list":["https://v.example.douyinvod.com/720.mp4"]}},{"play_addr":{"width":1080,"height":1920,"url_list":["https://v.example.douyinvod.com/1080.mp4"]}}]}}});
        let metadata = parse_douyin(&data, "1234567890123456789").unwrap();
        let result = build_resolved(&source, "1234567890123456789", metadata, 720).unwrap();
        assert_eq!(result.quality, 720);
        assert!(result.url.ends_with("720.mp4"));
        assert_eq!(result.info.qualities[0].label, "1080P");
        assert_eq!(result.info.duration, 12);
        assert!(parse_douyin(&data, "different-id").is_err());
    }
    #[test]
    fn xhs_state_handles_codec_fallback_backup_urls_and_rejects_galleries() {
        let id = "abcdef120000000012345678";
        let source = recognize(&format!("https://www.xiaohongshu.com/explore/{id}")).unwrap();
        let data = serde_json::json!({"note":{"noteDetailMap":{id:{"note":{"title":"视频笔记","user":{"nickname":"作者"},"video":{"media":{"stream":{"h264":[],"h265":[{"width":720,"height":1280,"duration":8000,"masterUrl":"https://untrusted.test/video.mp4","backupUrls":["https://sns-video-bd.xhscdn.com/video.mp4"]}]}}}}}}}});
        let metadata = parse_xhs(&data, id).unwrap();
        let result = build_resolved(&source, id, metadata, 0).unwrap();
        assert_eq!(result.quality, 720);
        assert_eq!(result.info.duration, 8);
        assert!(result.url.contains("xhscdn.com"));
        assert!(parse_xhs(&data, "different-id").is_err());
        let gallery = serde_json::json!({"note":{"noteDetailMap":{id:{"note":{"title":"图文"}}}}});
        assert!(build_resolved(&source, id, parse_xhs(&gallery, id).unwrap(), 0).is_err());
    }
    #[test]
    fn browser_fallback_requires_exact_source_and_direct_playable_media() {
        let source = recognize("https://www.douyin.com/video/1234567890123456789").unwrap();
        let mut data = serde_json::json!({"pageUrl":source.url,"title":"测试视频","player":{"url":"https://v.example.douyinvod.com/video.mp4","width":1080,"height":1920,"duration":15.5}});
        assert_eq!(from_browser(&source, &data, 0).unwrap().quality, 1080);
        data["pageUrl"] = "https://www.douyin.com/video/9876543210987654321".into();
        assert!(from_browser(&source, &data, 0).is_err());
        data["pageUrl"] = source.url.clone().into();
        data["player"]["url"] = "blob:https://www.douyin.com/fake".into();
        assert!(from_browser(&source, &data, 0).is_err());
    }
    #[test]
    fn browser_details_accept_official_play_redirect_and_alternate_avc_address() {
        let source = recognize("https://www.douyin.com/video/1234567890123456789").unwrap();
        let data = serde_json::json!({"pageUrl":source.url,"detail":{"aweme_id":"1234567890123456789","desc":"测试","video":{"width":1280,"height":720,"play_addr_h264":{"url_list":["https://www.douyin.com/aweme/v1/play/?video_id=fake"]}}},"player":{"url":"blob:https://www.douyin.com/fake"}});
        let result = from_browser(&source, &data, 0).unwrap();
        assert_eq!(result.quality, 720);
        assert!(result
            .url
            .starts_with("https://www.douyin.com/aweme/v1/play/"));
    }
    #[test]
    fn douyin_keeps_separate_audio_and_selects_a_trusted_highest_bitrate_url() {
        let source = recognize("https://www.douyin.com/video/1234567890123456789").unwrap();
        let data = serde_json::json!({"aweme_detail":{"aweme_id":"1234567890123456789","video":{"width":1280,"height":720,"play_addr":{"url_list":["https://v.example.douyinvod.com/video.mp4"]},"bit_rate_audio":[{"audio_meta":{"bitrate":64000,"url_list":{"main_url":"https://v.example.douyinvod.com/audio-low.m4a"}}},{"audio_meta":{"bitrate":128000,"url_list":{"main_url":"https://untrusted.test/audio","backup_url":"https://v.example.douyinvod.com/audio-high.m4a"}}}]}}});
        let resolved = from_detail(&source, &data, 0).unwrap();
        assert!(resolved.audio_url.unwrap().ends_with("audio-high.m4a"));
        assert!(resolved.url.ends_with("video.mp4"));
    }
    #[test]
    fn recognizes_share_text_and_strips_only_tracking() {
        let dy =
            recognize("复制打开抖音 https://www.douyin.com/jingxuan?modal_id=7689371847216893193")
                .unwrap();
        assert_eq!(dy.platform, Platform::Douyin);
        assert!(dy.url.ends_with("/video/7689371847216893193"));
        let xhs=recognize("看这里 https://www.xiaohongshu.com/explore/abcdef120000000012345678?xsec_token=test%3D&source=feed").unwrap();
        assert!(xhs.url.contains("xsec_token=test%3D"));
        assert!(!xhs.url.contains("source=feed"));
        assert!(
            !recognize("https://www.bilibili.com/video/BV1xx411c7mD?spm_id=tracking")
                .unwrap()
                .url
                .contains('?')
        );
        for u in [
            "https://douyin.com.evil.test/video/123456789",
            "http://localhost/video/1",
            "https://user@www.xiaohongshu.com/explore/abcdef120000000012345678",
        ] {
            assert!(recognize(u).is_err());
        }
    }
    #[test]
    fn desktop_recognizes_every_shared_plugin_platform_and_alias() {
        assert_eq!(catalog().len(), 50);
        for site in catalog() {
            for host in &site.hosts {
                assert_eq!(catalog_site(host).map(|s| s.id.as_str()), Some(site.id.as_str()));
                assert!(source_host(host).is_some(), "{}", host);
                assert!(source_host(&format!("{host}.evil.test")).is_none());
            }
        }
    }
    #[test]
    fn parses_json_without_executing_scripts() {
        let v = extract_state(
            r#"window.__INITIAL_STATE__={"a":undefined,"b":"undefined } \" ","c":{"d":1}};evil()"#,
            "window.__INITIAL_STATE__",
        )
        .unwrap();
        assert!(v["a"].is_null());
        assert!(v["b"].as_str().unwrap().starts_with("undefined"));
        assert_eq!(v["c"]["d"], 1);
    }
    #[test]
    fn rejects_untrusted_media_hosts() {
        assert!(media_url(
            Platform::Douyin,
            "https://www.douyin.com/aweme/v1/play/?video_id=test"
        )
        .is_ok());
        assert!(media_url(
            Platform::Douyin,
            "https://www.douyin.com/passport/web/login/"
        )
        .is_err());
        assert!(media_url(
            Platform::Xiaohongshu,
            "https://sns-video-bd.xhscdn.com/test.mp4"
        )
        .is_ok());
        assert!(media_url(Platform::Douyin, "https://v.example.douyinvod.com/test.mp4").is_ok());
        assert!(media_url(Platform::Douyin, "https://127.0.0.1/test").is_err());
    }
}
