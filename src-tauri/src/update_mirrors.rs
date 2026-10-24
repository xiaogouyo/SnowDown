use futures_util::{stream, StreamExt};
use reqwest::Client;
use serde::Serialize;
use std::time::{Duration, Instant};
use tokio::sync::watch;
use url::Url;

// Keep the user's complete library and stable one-based IDs. Availability and
// latency are measured for the current release, never copied from an old table.
pub const HOSTS: [&str; 66] = [
    "down.mxw.qzz.io",
    "tvv.tw",
    "xsadwsd.kdns.fr",
    "fastgit.cc",
    "gh.monlor.com",
    "ghproxy.imciel.com",
    "gh.acmsz.top",
    "gh.07150721.xyz",
    "cdn.akaere.online",
    "git.yylx.win",
    "down.mxw.xx.kg",
    "ghfast.top",
    "gh.my-website.ccwu.cc",
    "github.mxw.qzz.io",
    "gh-proxy.com",
    "ghproxy.felicity.land",
    "github.tbap.top",
    "gh.noki.icu",
    "cdn.gh-proxy.com",
    "gh.sixyin.com",
    "free.cn.eu.org",
    "git.669966.xyz",
    "gh.b52m.cn",
    "github.chenc.dev",
    "gh.dpik.top",
    "ghproxy.net",
    "g.blfrp.cn",
    "github.dpik.top",
    "ghfile.geekertao.top",
    "gh.bugdey.us.kg",
    "github-proxy.memory-echoes.cn",
    "gh.927223.xyz",
    "gh.inkchills.cn",
    "jiashu.1win.eu.org",
    "gh.felicity.ac.cn",
    "gitproxy.mrhjx.cn",
    "gh.ddlc.top",
    "ghf.xn--eqrr82bzpe.top",
    "gh.catmak.name",
    "gitproxy.127731.xyz",
    "ghpxy.hwinzniej.top",
    "ghm.078465.xyz",
    "gh.jasonzeng.dev",
    "gp.zkitefly.eu.org",
    "github.geekery.cn",
    "github.ednovas.xyz",
    "ghproxy.1888866.xyz",
    "ghproxy.cxkpro.top",
    "gh.idayer.com",
    "gh.chjina.com",
    "ghp.keleyaa.com",
    "ghp.arslantu.xyz",
    "githubdog.com",
    "gh.meali.top",
    "777.z321.cc.cd",
    "g.z321.cc.cd",
    "gg.z321.cc.cd",
    "js.jiangss.shop",
    "github.ikgy.top",
    "cfgh.ikgy.top",
    "ghproxy.monkeyray.net",
    "gh.qfmc0721.cc.cd",
    "github.xxlab.tech",
    "proxy.yaoyaoling.net",
    "gh.ruan.dpdns.org",
    "github.nswrz.cn",
];

const PROBE_BYTES: usize = 4096;
const PROBE_CONCURRENCY: usize = 12;
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MirrorState {
    Pending,
    Testing,
    Available,
    Timeout,
    Unsupported,
    Unavailable,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MirrorResult {
    pub id: usize,
    pub host: String,
    pub latency_ms: Option<u64>,
    pub state: MirrorState,
    pub message: Option<String>,
}

pub fn pending() -> Vec<MirrorResult> {
    HOSTS
        .iter()
        .enumerate()
        .map(|(index, host)| MirrorResult {
            id: index + 1,
            host: (*host).into(),
            latency_ms: None,
            state: MirrorState::Pending,
            message: None,
        })
        .collect()
}

pub fn is_mirror_host(host: &str) -> bool {
    HOSTS.contains(&host)
}

pub fn proxy_url(host: &str, github_url: &str) -> String {
    format!("https://{host}/{github_url}")
}

fn allowed_redirect(url: &Url) -> bool {
    url.scheme() == "https"
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && url.host_str().is_some_and(|host| {
            is_mirror_host(host)
                || matches!(
                    host,
                    "github.com"
                        | "release-assets.githubusercontent.com"
                        | "objects.githubusercontent.com"
                )
        })
}

pub fn client() -> Result<Client, String> {
    Client::builder()
        .user_agent(concat!("SnowDown/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(PROBE_TIMEOUT)
        .read_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(900))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() < 6 && allowed_redirect(attempt.url()) {
                attempt.follow()
            } else {
                attempt.error("更新镜像跳转到不支持的地址")
            }
        }))
        .build()
        .map_err(|_| "无法初始化镜像连接。".into())
}

fn valid_range(value: Option<&str>, size: u64) -> bool {
    let Some(value) = value.and_then(|value| value.strip_prefix("bytes ")) else {
        return false;
    };
    let Some((range, total)) = value.split_once('/') else {
        return false;
    };
    let Some((start, end)) = range.split_once('-') else {
        return false;
    };
    start == "0"
        && total.parse::<u64>().ok() == Some(size)
        && end
            .parse::<u64>()
            .ok()
            .is_some_and(|end| end >= 1 && end < PROBE_BYTES as u64 && end < size)
}

async fn probe(client: &Client, url: &str, size: u64) -> Result<u64, (MirrorState, String)> {
    let began = Instant::now();
    let mut response = client
        .get(url)
        .header("Range", format!("bytes=0-{}", PROBE_BYTES - 1))
        .header("Accept-Encoding", "identity")
        .send()
        .await
        .map_err(|error| {
            (
                if error.is_timeout() {
                    MirrorState::Timeout
                } else {
                    MirrorState::Unavailable
                },
                "无法连接或证书/跳转无效".into(),
            )
        })?;
    let partial = response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
    let valid_headers = if partial {
        valid_range(
            response
                .headers()
                .get("content-range")
                .and_then(|value| value.to_str().ok()),
            size,
        )
    } else {
        response.status() == reqwest::StatusCode::OK && response.content_length() == Some(size)
    };
    if !valid_headers {
        return Err((
            MirrorState::Unsupported,
            "未返回当前更新包或大小/分段信息不匹配".into(),
        ));
    }
    let mut prefix = Vec::with_capacity(PROBE_BYTES);
    let mut latency = None;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| (MirrorState::Unavailable, "读取更新包失败".into()))?
    {
        if chunk.is_empty() {
            continue;
        }
        let count = chunk.len().min(PROBE_BYTES - prefix.len());
        prefix.extend_from_slice(&chunk[..count]);
        if prefix.len() >= 2 {
            if !prefix.starts_with(b"MZ") {
                return Err((
                    MirrorState::Unsupported,
                    "返回内容不是 Windows 更新包".into(),
                ));
            }
            latency.get_or_insert(began.elapsed().as_millis() as u64);
        }
        if prefix.len() >= PROBE_BYTES || !partial {
            break;
        }
    }
    latency.ok_or((MirrorState::Unsupported, "更新包内容为空或不完整".into()))
}

pub async fn probe_all(
    client: &Client,
    github_url: &str,
    size: u64,
    mut cancel: watch::Receiver<u64>,
    emit: impl Fn(MirrorResult),
) -> Result<Vec<MirrorResult>, String> {
    if cancel.has_changed().unwrap_or(false) {
        return Err("已取消下载更新。".into());
    }
    let jobs = stream::iter(pending())
        .map(|mut result| {
            let emit = &emit;
            async move {
                result.state = MirrorState::Testing;
                emit(result.clone());
                let url = proxy_url(&result.host, github_url);
                match tokio::time::timeout(PROBE_TIMEOUT, probe(client, &url, size)).await {
                    Ok(Ok(latency)) => {
                        result.state = MirrorState::Available;
                        result.latency_ms = Some(latency);
                    }
                    Ok(Err((state, message))) => {
                        result.state = state;
                        result.message = Some(message);
                    }
                    Err(_) => {
                        result.state = MirrorState::Timeout;
                        result.message = Some("5 秒内未取得有效响应".into());
                    }
                }
                emit(result.clone());
                result
            }
        })
        .buffer_unordered(PROBE_CONCURRENCY);
    futures_util::pin_mut!(jobs);
    let mut results = Vec::with_capacity(HOSTS.len());
    loop {
        let next = tokio::select! {
            _ = cancel.changed() => return Err("已取消下载更新。".into()),
            next = jobs.next() => next,
        };
        match next {
            Some(result) => results.push(result),
            None => break,
        }
    }
    results.sort_by_key(|result| result.id);
    Ok(results)
}

pub fn ranked(results: &[MirrorResult]) -> Vec<&MirrorResult> {
    let mut available = results
        .iter()
        .filter(|result| result.state == MirrorState::Available && result.latency_ms.is_some())
        .collect::<Vec<_>>();
    available.sort_by_key(|result| (result.latency_ms.unwrap_or(u64::MAX), result.id));
    available
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn complete_library_preserves_all_66_addresses_and_ids() {
        let rows = pending();
        assert_eq!(rows.len(), 66);
        assert_eq!(
            HOSTS.iter().collect::<std::collections::HashSet<_>>().len(),
            66
        );
        assert!(rows
            .iter()
            .enumerate()
            .all(|(index, result)| result.id == index + 1 && result.host == HOSTS[index]));
        assert_eq!(rows[0].host, "down.mxw.qzz.io");
        assert_eq!(rows[60].host, "ghproxy.monkeyray.net");
        assert_eq!(rows[65].host, "github.nswrz.cn");
        assert!(HOSTS
            .iter()
            .all(
                |host| Url::parse(&format!("https://{host}/")).unwrap().host_str() == Some(*host)
            ));
    }
    #[test]
    fn only_real_available_results_participate_in_latency_ranking() {
        let mut rows = pending();
        for (index, latency) in [(0, 438), (1, 173), (5, 112)] {
            rows[index].state = MirrorState::Available;
            rows[index].latency_ms = Some(latency);
        }
        rows[2].state = MirrorState::Failed;
        rows[2].latency_ms = Some(1);
        assert_eq!(
            ranked(&rows)
                .iter()
                .map(|result| result.id)
                .collect::<Vec<_>>(),
            [6, 2, 1]
        );
        assert!(!allowed_redirect(
            &Url::parse("https://gh-proxy.com.evil.test/file").unwrap()
        ));
        assert!(!allowed_redirect(
            &Url::parse("http://gh-proxy.com/file").unwrap()
        ));
        assert!(!allowed_redirect(
            &Url::parse("https://user@gh-proxy.com/file").unwrap()
        ));
        assert!(allowed_redirect(
            &Url::parse("https://gh-proxy.com/file").unwrap()
        ));
    }
    #[test]
    fn range_validation_rejects_wrong_file_size_and_offsets() {
        assert!(valid_range(Some("bytes 0-4095/32868864"), 32868864));
        for value in [
            "bytes 1-4095/32868864",
            "bytes 0-4095/10",
            "bytes 0-99999/32868864",
            "bytes 0-4095/*",
        ] {
            assert!(!valid_range(Some(value), 32868864));
        }
    }

    #[tokio::test]
    async fn probe_rejects_html_and_accepts_only_a_matching_executable() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        for (body, status, range, accepted) in [
            (
                b"MZtest".as_slice(),
                "206 Partial Content",
                "Content-Range: bytes 0-5/100\r\n",
                true,
            ),
            (
                b"<html>".as_slice(),
                "206 Partial Content",
                "Content-Range: bytes 0-5/100\r\n",
                false,
            ),
            (
                b"MZtest".as_slice(),
                "206 Partial Content",
                "Content-Range: bytes 0-5/99\r\n",
                false,
            ),
            (b"MZtest".as_slice(), "200 OK", "", false),
        ] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("http://{}/fixture", listener.local_addr().unwrap());
            let task = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = [0; 4096];
                let count = socket.read(&mut request).await.unwrap();
                assert!(String::from_utf8_lossy(&request[..count])
                    .to_lowercase()
                    .contains("range: bytes=0-4095"));
                socket.write_all(format!("HTTP/1.1 {status}\r\n{range}Content-Length: {}\r\nConnection: close\r\n\r\n",body.len()).as_bytes()).await.unwrap();
                socket.write_all(body).await.unwrap();
            });
            assert_eq!(probe(&Client::new(), &url, 100).await.is_ok(), accepted);
            task.await.unwrap();
        }
    }

    #[tokio::test]
    async fn cancelled_scan_does_not_launch_network_requests() {
        let (sender, receiver) = watch::channel(0);
        sender.send_replace(1);
        assert!(
            probe_all(&Client::new(), "unused", 100, receiver, |_| panic!(
                "already cancelled"
            ))
            .await
            .unwrap_err()
            .contains("取消")
        );
    }
}
