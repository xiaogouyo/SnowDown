use crate::model::{Quality, VideoInfo, VideoPage};
use reqwest::{header, Client};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use url::Url;

pub const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

pub fn client(cookie: &str) -> Result<Client, String> {
    let mut headers = header::HeaderMap::new();
    headers.insert(
        header::REFERER,
        header::HeaderValue::from_static("https://www.bilibili.com/"),
    );
    if !cookie.is_empty() {
        headers.insert(
            header::COOKIE,
            header::HeaderValue::from_str(cookie).map_err(|_| "登录凭据格式不正确")?,
        );
    }
    Client::builder()
        .user_agent(USER_AGENT)
        .default_headers(headers)
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(45))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|e| e.to_string())
}

pub fn normalize_input(input: &str) -> Result<(String, u32), String> {
    let input = input.trim();
    if input.starts_with("BV")
        && input.len() == 12
        && input.chars().all(|c| c.is_ascii_alphanumeric())
    {
        return Ok((input.into(), 1));
    }
    if input.to_ascii_lowercase().starts_with("av")
        && input.len() > 2
        && input[2..].chars().all(|c| c.is_ascii_digit())
    {
        return Ok((input.to_ascii_lowercase(), 1));
    }
    let start = input
        .find("https://")
        .or_else(|| input.find("http://"))
        .ok_or("请输入B站视频链接、BV号或av号。")?;
    let raw = input[start..]
        .split(|c: char| c.is_whitespace() || "，。！？）】>\"".contains(c))
        .next()
        .unwrap_or("");
    let url = Url::parse(raw).map_err(|_| "链接格式不正确")?;
    let host = url.host_str().unwrap_or("");
    if !(host == "bilibili.com" || host.ends_with(".bilibili.com") || host == "b23.tv") {
        return Err("首版支持 bilibili.com 和 b23.tv 的视频链接。".into());
    }
    if url.username() != "" || url.password().is_some() || url.port().is_some() {
        return Err("视频链接不能包含用户名、密码或自定义端口。".into());
    }
    if host == "b23.tv" {
        return Ok((url.to_string(), 1));
    }
    let id = url
        .path_segments()
        .and_then(|segments| {
            segments
                .filter(|s| s.starts_with("BV") || s.to_ascii_lowercase().starts_with("av"))
                .next()
        })
        .ok_or("暂不支持此链接，请使用 /video/BV… 视频页面链接。")?;
    let (id, _) = normalize_input(id)?;
    let page = url
        .query_pairs()
        .find(|(key, _)| key == "p")
        .and_then(|(_, v)| v.parse::<u32>().ok())
        .unwrap_or(1)
        .max(1);
    Ok((id, page))
}

async fn api(client: &Client, url: &str, query: &[(&str, String)]) -> Result<Value, String> {
    let response = client
        .get(url)
        .query(query)
        .send()
        .await
        .map_err(|e| format!("连接B站失败：{e}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "B站服务返回 HTTP {}，请稍后重试。",
            response.status()
        ));
    }
    let body: Value = response
        .json()
        .await
        .map_err(|_| "B站返回了无法识别的数据，请稍后重试。")?;
    if body["code"].as_i64() != Some(0) {
        let code = body["code"].as_i64().unwrap_or(-1);
        let message = body["message"].as_str().unwrap_or("请求未成功");
        return Err(match code {
            -404 | 62002 | 62004 => "视频不存在、已被删除，或当前账号无法访问。".into(),
            -101 | -104 => "该视频需要登录，请在设置中添加B站登录凭据。".into(),
            -352 | -412 | -509 => "B站暂时限制了请求，请降低并发并稍后重试。".into(),
            _ => format!("B站返回 {code}：{message}"),
        });
    }
    Ok(body["data"].clone())
}

pub async fn inspect(input: &str, cookie: &str) -> Result<VideoInfo, String> {
    let client = client(cookie)?;
    let (mut id, mut selected_page) = normalize_input(input)?;
    if id.starts_with("https://") || id.starts_with("http://") {
        let short_client = Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                let host = attempt.url().host_str().unwrap_or("");
                if attempt.previous().len() > 5
                    || !(host == "b23.tv"
                        || host == "bilibili.com"
                        || host.ends_with(".bilibili.com"))
                {
                    attempt.stop()
                } else {
                    attempt.follow()
                }
            }))
            .build()
            .map_err(|e| e.to_string())?;
        let response = short_client
            .get(&id)
            .send()
            .await
            .map_err(|e| format!("短链接解析失败：{e}"))?;
        (id, selected_page) = normalize_input(response.url().as_str())?;
        if id.starts_with("http") {
            return Err("短链接没有跳转到可下载的视频页面。".into());
        }
    }
    let query = if id.starts_with("BV") {
        vec![("bvid", id.clone())]
    } else {
        vec![("aid", id[2..].into())]
    };
    let data = api(
        &client,
        "https://api.bilibili.com/x/web-interface/view",
        &query,
    )
    .await?;
    let pages: Vec<VideoPage> =
        serde_json::from_value(data["pages"].clone()).map_err(|_| "视频分P信息不完整")?;
    let cid = pages
        .iter()
        .find(|page| page.page == selected_page)
        .or(pages.first())
        .ok_or("视频没有可下载的分P")?
        .cid;
    let bvid = data["bvid"].as_str().ok_or("视频标识缺失")?.to_string();
    let play = play_info(&client, &bvid, cid, 127).await?;
    Ok(VideoInfo {
        platform: crate::model::Platform::Bilibili,
        source_url: format!("https://www.bilibili.com/video/{bvid}?p={selected_page}"),
        bvid,
        title: data["title"].as_str().unwrap_or("未命名视频").into(),
        cover: data["pic"]
            .as_str()
            .unwrap_or("")
            .replace("http://", "https://"),
        author: data["owner"]["name"].as_str().unwrap_or("未知UP主").into(),
        duration: data["duration"].as_u64().unwrap_or(0),
        pages,
        selected_cid: cid,
        qualities: qualities(&play),
    })
}

pub async fn play_info(
    client: &Client,
    bvid: &str,
    cid: u64,
    quality: u32,
) -> Result<Value, String> {
    let mut params = BTreeMap::from([
        ("bvid".into(), bvid.to_string()),
        ("cid".into(), cid.to_string()),
        ("qn".into(), quality.to_string()),
        ("fnval".into(), "4048".into()),
        ("fnver".into(), "0".into()),
        ("fourk".into(), "1".into()),
    ]);
    let nav = api(client, "https://api.bilibili.com/x/web-interface/nav", &[]).await;
    if let Ok(nav) = nav {
        if let (Some(img), Some(sub)) = (
            nav["wbi_img"]["img_url"].as_str(),
            nav["wbi_img"]["sub_url"].as_str(),
        ) {
            let key = format!("{}{}", image_key(img), image_key(sub));
            sign_wbi(&mut params, &key);
            let query: Vec<(&str, String)> = params
                .iter()
                .map(|(k, v)| (k.as_str(), v.clone()))
                .collect();
            if let Ok(data) = api(
                client,
                "https://api.bilibili.com/x/player/wbi/playurl",
                &query,
            )
            .await
            {
                return validate_play(data);
            }
        }
    }
    params.remove("w_rid");
    params.remove("wts");
    let query: Vec<(&str, String)> = params
        .iter()
        .map(|(k, v)| (k.as_str(), v.clone()))
        .collect();
    validate_play(api(client, "https://api.bilibili.com/x/player/playurl", &query).await?)
}

fn validate_play(data: Value) -> Result<Value, String> {
    if data["dash"]["video"]
        .as_array()
        .is_some_and(|items| !items.is_empty())
        || data["durl"]
            .as_array()
            .is_some_and(|items| !items.is_empty())
    {
        Ok(data)
    } else {
        Err("当前账号没有取得可下载的音视频资源，请检查登录状态。".into())
    }
}

fn image_key(url: &str) -> &str {
    url.rsplit('/')
        .next()
        .unwrap_or("")
        .split('.')
        .next()
        .unwrap_or("")
}

fn sign_wbi(params: &mut BTreeMap<String, String>, key: &str) {
    const MIX: [usize; 64] = [
        46, 47, 18, 2, 53, 8, 23, 32, 15, 50, 10, 31, 58, 3, 45, 35, 27, 43, 5, 49, 33, 9, 42, 19,
        29, 28, 14, 39, 12, 38, 41, 13, 37, 48, 7, 16, 24, 55, 40, 61, 26, 17, 0, 1, 60, 51, 30, 4,
        22, 25, 54, 21, 56, 59, 6, 63, 57, 62, 11, 36, 20, 34, 44, 52,
    ];
    let mixed: String = MIX
        .iter()
        .filter_map(|i| key.as_bytes().get(*i).copied().map(char::from))
        .take(32)
        .collect();
    params.insert(
        "wts".into(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .to_string(),
    );
    for value in params.values_mut() {
        *value = value.chars().filter(|c| !"!'()*".contains(*c)).collect();
    }
    let query = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(params.iter())
        .finish();
    params.insert(
        "w_rid".into(),
        format!("{:x}", md5::compute(format!("{query}{mixed}"))),
    );
}

pub fn quality_label(id: u32) -> &'static str {
    match id {
        127 => "8K 超高清",
        126 => "杜比视界",
        125 => "HDR 真彩",
        120 => "4K 超高清",
        116 => "1080P 60帧",
        112 => "1080P 高码率",
        80 => "1080P 高清",
        74 => "720P 60帧",
        64 => "720P 高清",
        32 => "480P 清晰",
        16 => "360P 流畅",
        _ => "原始画质",
    }
}

pub fn qualities(data: &Value) -> Vec<Quality> {
    let mut map = BTreeMap::new();
    if let Some(videos) = data["dash"]["video"].as_array() {
        for video in videos {
            let id = video["id"].as_u64().unwrap_or(0) as u32;
            map.entry(id).or_insert(Quality {
                id,
                label: quality_label(id).into(),
                width: video["width"].as_u64().unwrap_or(0),
                height: video["height"].as_u64().unwrap_or(0),
            });
        }
    }
    if map.is_empty() {
        let id = data["quality"].as_u64().unwrap_or(32) as u32;
        map.insert(
            id,
            Quality {
                id,
                label: quality_label(id).into(),
                width: 0,
                height: 0,
            },
        );
    }
    map.into_values().rev().collect()
}

pub fn media_url(stream: &Value) -> Result<String, String> {
    let raw = stream["baseUrl"]
        .as_str()
        .or(stream["base_url"].as_str())
        .or(stream["url"].as_str())
        .ok_or("媒体地址缺失")?;
    let raw = if raw.starts_with("//") {
        format!("https:{raw}")
    } else {
        raw.to_string()
    };
    let url = Url::parse(&raw).map_err(|_| "媒体地址无效")?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("媒体协议不受支持".into());
    }
    Ok(raw)
}

pub async fn subtitles(
    client: &Client,
    bvid: &str,
    cid: u64,
) -> Result<Vec<(String, String)>, String> {
    let data = api(
        client,
        "https://api.bilibili.com/x/player/v2",
        &[("bvid", bvid.into()), ("cid", cid.to_string())],
    )
    .await?;
    let mut result = vec![];
    let subtitle_client = self::client("")?;
    if let Some(items) = data["subtitle"]["subtitles"].as_array() {
        for item in items {
            if let Some(raw) = item["subtitle_url"].as_str() {
                let url = if raw.starts_with("//") {
                    format!("https:{raw}")
                } else {
                    raw.into()
                };
                let host = Url::parse(&url)
                    .ok()
                    .and_then(|u| u.host_str().map(str::to_string))
                    .unwrap_or_default();
                if !(host.ends_with(".hdslb.com") || host.ends_with(".bilibili.com")) {
                    continue;
                }
                let body: Value = subtitle_client
                    .get(url)
                    .send()
                    .await
                    .map_err(|e| e.to_string())?
                    .error_for_status()
                    .map_err(|e| e.to_string())?
                    .json()
                    .await
                    .map_err(|e| e.to_string())?;
                let mut srt = String::new();
                if let Some(lines) = body["body"].as_array() {
                    for (index, line) in lines.iter().enumerate() {
                        srt.push_str(&format!(
                            "{}\n{} --> {}\n{}\n\n",
                            index + 1,
                            srt_time(line["from"].as_f64().unwrap_or(0.0)),
                            srt_time(line["to"].as_f64().unwrap_or(0.0)),
                            line["content"].as_str().unwrap_or("")
                        ));
                    }
                }
                result.push((item["lan"].as_str().unwrap_or("subtitle").into(), srt));
            }
        }
    }
    Ok(result)
}

fn srt_time(seconds: f64) -> String {
    let ms = (seconds.max(0.0) * 1000.0).round() as u64;
    format!(
        "{:02}:{:02}:{:02},{:03}",
        ms / 3600000,
        ms / 60000 % 60,
        ms / 1000 % 60,
        ms % 1000
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_and_normalizes_inputs() {
        assert_eq!(
            normalize_input(" BV1xx411c7mD ").unwrap(),
            ("BV1xx411c7mD".into(), 1)
        );
        assert_eq!(
            normalize_input("【分享】 https://www.bilibili.com/video/BV1xx411c7mD/?p=2 视频")
                .unwrap()
                .1,
            2
        );
        assert_eq!(normalize_input("AV170001").unwrap().0, "av170001");
        assert!(normalize_input("https://bilibili.com.attacker.test/video/BV1xx411c7mD").is_err());
        assert!(normalize_input("https://127.0.0.1/video/BV1xx411c7mD").is_err());
        assert!(normalize_input("https://www.bilibili.com:443/video/BV1xx411c7mD").is_ok());
        assert!(normalize_input("BV../../evil").is_err());
    }
    #[test]
    fn only_available_qualities_are_displayed() {
        let data = serde_json::json!({"accept_quality":[80,32],"dash":{"video":[{"id":32,"width":852,"height":480},{"id":32,"width":852,"height":480}]}});
        assert_eq!(qualities(&data).len(), 1);
        assert_eq!(qualities(&data)[0].id, 32);
    }
    #[test]
    fn subtitle_timestamps_round_correctly() {
        assert_eq!(srt_time(3661.123), "01:01:01,123");
    }

    #[tokio::test]
    #[ignore = "Uses the live public Bilibili API; run explicitly during release verification"]
    async fn live_bilibili_parse() {
        let info = inspect("BV1xx411c7mD", "").await.unwrap();
        assert!(!info.pages.is_empty());
        assert!(!info.qualities.is_empty());
        eprintln!(
            "Live API OK: {} / {} pages / {:?}",
            info.title,
            info.pages.len(),
            info.qualities.iter().map(|q| &q.label).collect::<Vec<_>>()
        );
        let api_client = client("").unwrap();
        let play = play_info(&api_client, &info.bvid, info.selected_cid, 32)
            .await
            .unwrap();
        let stream = play["dash"]["video"]
            .as_array()
            .and_then(|items| items.first())
            .or_else(|| play["durl"].as_array().and_then(|items| items.first()))
            .unwrap();
        let media_client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap();
        let response = media_client
            .get(media_url(stream).unwrap())
            .header(header::REFERER, "https://www.bilibili.com/")
            .header(header::RANGE, "bytes=0-65535")
            .send()
            .await
            .unwrap();
        assert!(
            response.status().is_success(),
            "Media response: {}",
            response.status()
        );
        let mut stream = response.bytes_stream();
        use futures_util::StreamExt;
        let chunk = stream.next().await.unwrap().unwrap();
        assert!(!chunk.is_empty());
        eprintln!("Live CDN OK: received a {} byte media chunk", chunk.len());
    }
}
