use crate::{model::*, AppState};
use std::{net::IpAddr, sync::atomic::AtomicBool, time::Duration};
use tauri::AppHandle;

pub fn public_address(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            !ip.is_private()
                && !ip.is_loopback()
                && !ip.is_link_local()
                && !ip.is_unspecified()
                && !ip.is_broadcast()
                && !ip.is_multicast()
                && ip.octets()[0] != 0
                && ip.octets()[0] < 224
                && !(ip.octets()[0] == 100 && (64..128).contains(&ip.octets()[1]))
        }
        IpAddr::V6(ip) => ip
            .to_ipv4_mapped()
            .map(|ip| public_address(ip.into()))
            .unwrap_or_else(|| {
                !ip.is_loopback()
                    && !ip.is_unspecified()
                    && !ip.is_multicast()
                    && (ip.segments()[0] & 0xfe00) != 0xfc00
                    && (ip.segments()[0] & 0xffc0) != 0xfe80
            }),
    }
}
pub fn validate(url: &str) -> Result<url::Url, String> {
    let url = url::Url::parse(url).map_err(|_| "资源地址无效。")?;
    if url.scheme() != "https"
        || url.username() != ""
        || url.password().is_some()
        || url.port().is_some()
        || url.host_str().is_none_or(|h| {
            h == "localhost"
                || h.ends_with(".local")
                || h.ends_with(".localhost")
                || h.trim_matches(['[', ']'])
                    .parse::<IpAddr>()
                    .is_ok_and(|ip| !public_address(ip))
        })
    {
        return Err("桌面端仅接收公开 HTTPS 资源，请改用浏览器下载此文件。".into());
    }
    Ok(url)
}
pub fn enqueue(
    app: &AppHandle,
    state: &AppState,
    url: &str,
    title: &str,
    kind: &str,
) -> Result<(), String> {
    let parsed = validate(url)?;
    let extension = parsed
        .path()
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_lowercase();
    if ![
        "png", "jpg", "jpeg", "webp", "gif", "avif", "bmp", "svg", "mp4", "webm", "mov", "m4v",
        "mp3", "m4a", "aac", "ogg", "wav",
    ]
    .contains(&extension.as_str())
        || !["image", "video", "audio"].contains(&kind)
    {
        return Err("此资源地址没有可确认的文件格式，请用浏览器下载。".into());
    }
    let id = uuid::Uuid::new_v4().to_string();
    let settings = state.settings.lock().unwrap().clone();
    let mut tasks = state.tasks.lock().unwrap();
    if state.updater.installing() {
        return Err("正在安装更新。".into());
    }
    if tasks.iter().any(|t| {
        t.resource.as_ref().is_some_and(|r| r.url == url)
            && matches!(
                t.status,
                TaskStatus::Queued | TaskStatus::Downloading | TaskStatus::Paused
            )
    }) {
        return Ok(());
    }
    let task:DownloadTask=serde_json::from_value(serde_json::json!({"resource":{"url":url,"extension":extension},"platform":"bilibili","sourceUrl":url,"id":id,"bvid":"browser-resource","cid":0,"title":safe_filename(title),"pageTitle":"浏览器资源","page":1,"cover":"","quality":0,"qualityLabel":"原始资源","format":if kind=="audio"{"audio"}else{"video"},"status":"queued","progress":0,"downloadedBytes":0,"totalBytes":0,"speed":0,"error":null,"outputPath":null,"createdAt":now_ms(),"downloadDir":settings.download_dir,"saveCover":false,"saveDanmaku":false,"saveSubtitles":false,"warning":null})).map_err(|_|"无法创建资源任务。")?;
    std::fs::create_dir_all(&task.download_dir).map_err(|_| "无法创建下载目录。")?;
    state.storage.save_task(&task)?;
    tasks.push(task);
    drop(tasks);
    state.emit(app);
    Ok(())
}
pub async fn run(
    app: &AppHandle,
    state: &AppState,
    task: &DownloadTask,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let resource = task.resource.as_ref().ok_or("资源任务无效。")?;
    let url = validate(&resource.url)?;
    let host = url.host_str().unwrap();
    let addresses = tokio::net::lookup_host((host, 443))
        .await
        .map_err(|_| "无法解析资源服务器。")?
        .collect::<Vec<_>>();
    if addresses.is_empty() || addresses.iter().any(|a| !public_address(a.ip())) {
        return Err("资源服务器不是公开地址。".into());
    }
    // Pin the validated addresses; no rebinding or cross-host redirect can reach local services.
    let client = reqwest::Client::builder()
        .http1_only()
        .resolve_to_addrs(host, &addresses)
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(35))
        .build()
        .map_err(|_| "无法初始化资源下载。")?;
    let directory = std::path::PathBuf::from(&task.download_dir);
    let temp = directory.join(".snowdown").join(&task.id);
    tokio::fs::create_dir_all(&temp)
        .await
        .map_err(|_| "无法创建临时目录。")?;
    let part = temp.join("resource.part");
    let settings = state.settings.lock().unwrap().clone();
    let outcome = crate::transfer::download(
        &client,
        &resource.url,
        &part,
        crate::transfer::Options {
            identity: &task.id,
            expected_size: None,
            sha256: None,
            connections: settings.download_connections,
            max_bytes_per_second: settings.max_speed_mbps as u64 * 1024 * 1024,
        },
        cancel,
        |stats| {
            let _ = state.patch_task(app, &task.id, |t| {
                if !cancel.load(std::sync::atomic::Ordering::Relaxed) {
                    t.total_bytes = stats.total;
                    t.downloaded_bytes = stats.downloaded;
                    t.progress = if stats.total > 0 {
                        stats.downloaded as f64 / stats.total as f64 * 99.
                    } else {
                        0.
                    };
                    t.speed = stats.speed;
                    t.active_connections = stats.active_connections;
                    t.resumed_bytes = stats.resumed_bytes;
                }
            });
        },
    )
    .await?;
    if cancel.load(std::sync::atomic::Ordering::Relaxed) {
        return Err("下载已暂停。".into());
    }
    let output = directory.join(format!(
        "{}-{}.{}",
        safe_filename(&task.title),
        &task.id[..8],
        resource.extension
    ));
    tokio::fs::rename(&part, &output)
        .await
        .map_err(|_| "无法保存资源文件。")?;
    crate::transfer::cleanup(&part).await;
    let _ = tokio::fs::remove_dir(&temp).await;
    state.patch_task(app, &task.id, |t| {
        t.status = TaskStatus::Completed;
        t.progress = 100.;
        t.total_bytes = outcome.bytes;
        t.downloaded_bytes = outcome.bytes;
        t.peak_connections = outcome.peak_connections;
        t.output_path = Some(output.to_string_lossy().into_owned());
        t.error = None;
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_local_and_unsafe_sources() {
        for value in [
            "http://example.com/a.mp4",
            "https://127.0.0.1/a.mp4",
            "https://[::1]/a",
            "https://100.64.0.1/x",
            "https://user:pass@example.com/a",
            "file:///C:/a",
            "https://host.local/a",
        ] {
            assert!(validate(value).is_err(), "{value}");
        }
        assert!(validate("https://example.com/a.mp4").is_ok());
    }
}
