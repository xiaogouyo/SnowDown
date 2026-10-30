use crate::{model::*, AppState};
use std::{
    io::Write,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use tauri::AppHandle;

pub fn validate_items(items: &[ArchiveResource]) -> Result<(), String> {
    if items.is_empty() || items.len() > 200 {
        return Err("每个 ZIP 需要 1 至 200 个资源。".into());
    }
    let mut names = std::collections::HashSet::new();
    for item in items {
        crate::resource_download::validate(&item.url)?;
        if item.url.len() > 4096
            || item.name.is_empty()
            || item.name.len() > 240
            || item.name == "."
            || item.name == ".."
            || item
                .name
                .chars()
                .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
            || !names.insert(item.name.to_lowercase())
        {
            return Err("ZIP 文件名无效或重复。".into());
        }
        let path = url::Url::parse(&item.url)
            .map_err(|_| "资源地址无效。")?
            .path()
            .to_lowercase();
        if [".m3u8", ".mpd", ".m4s", ".ts"]
            .iter()
            .any(|ext| path.ends_with(ext))
        {
            return Err("请选择完整文件，不能直接打包播放清单或分片。".into());
        }
    }
    Ok(())
}
pub fn enqueue(
    app: &AppHandle,
    state: &AppState,
    items: Vec<ArchiveResource>,
    name: &str,
) -> Result<(), String> {
    validate_items(&items)?;
    if state.updater.installing() {
        return Err("正在安装更新。".into());
    }
    let settings = state.settings.lock().unwrap().clone();
    let id = uuid::Uuid::new_v4().to_string();
    let task:DownloadTask=serde_json::from_value(serde_json::json!({"resource":{"url":items[0].url,"extension":"zip","archive":items},"platform":"web","sourceUrl":items[0].url,"id":id,"bvid":"browser-archive","cid":0,"title":safe_filename(name),"pageTitle":"网页资源 ZIP","page":1,"cover":"","quality":0,"qualityLabel":"ZIP 打包","format":"video","status":"queued","progress":0,"downloadedBytes":0,"totalBytes":0,"speed":0,"error":null,"outputPath":null,"createdAt":now_ms(),"downloadDir":settings.download_dir,"saveCover":false,"saveDanmaku":false,"saveSubtitles":false,"warning":null})).map_err(|_|"无法创建打包任务。")?;
    state.storage.save_task(&task)?;
    state.tasks.lock().unwrap().push(task);
    state.emit(app);
    Ok(())
}
pub async fn run(
    app: &AppHandle,
    state: &AppState,
    task: &DownloadTask,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let items = &task.resource.as_ref().ok_or("打包任务无效。")?.archive;
    validate_items(items)?;
    let directory = std::path::PathBuf::from(&task.download_dir);
    let temp = directory.join(".snowdown").join(&task.id);
    tokio::fs::create_dir_all(&temp)
        .await
        .map_err(|_| "无法创建打包目录。")?;
    let settings = state.settings.lock().unwrap().clone();
    let mut completed = 0u64;
    let mut files = vec![];
    let proxy = crate::extractor_proxy::start().await?;
    for (i, item) in items.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err("打包已暂停，文件进度已保留。".into());
        }
        let url = crate::resource_download::validate(&item.url)?;
        let host = url.host_str().unwrap();
        let addresses = tokio::net::lookup_host((host, 443))
            .await
            .map_err(|_| "无法解析资源服务器。")?
            .collect::<Vec<_>>();
        if addresses.is_empty()
            || addresses
                .iter()
                .any(|a| !crate::resource_download::public_address(a.ip()))
        {
            return Err("资源不是公开地址。".into());
        }
        let client = reqwest::Client::builder()
            .proxy(reqwest::Proxy::all(&proxy.url).map_err(|_| "资源连接不可用。")?)
            .http1_only()
            .resolve_to_addrs(host, &addresses)
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(15))
            .read_timeout(Duration::from_secs(35))
            .build()
            .map_err(|_| "无法初始化资源下载。")?;
        let part = temp.join(format!("item-{i}.part"));
        let identity = format!("{}:{i}", task.id);
        let outcome = crate::transfer::download(
            &client,
            &item.url,
            &part,
            crate::transfer::Options {
                identity: &identity,
                expected_size: None,
                sha256: None,
                connections: settings.download_connections,
                max_bytes_per_second: settings.max_speed_mbps as u64 * 1024 * 1024,
            },
            cancel,
            |stats| {
                let _ = state.patch_task(app, &task.id, |t| {
                    t.downloaded_bytes = completed + stats.downloaded;
                    t.speed = stats.speed;
                    t.progress = (i as f64
                        + if stats.total > 0 {
                            stats.downloaded as f64 / stats.total as f64
                        } else {
                            0.
                        })
                        / items.len() as f64
                        * 90.;
                });
            },
        )
        .await?;
        completed += outcome.bytes;
        if completed > 2 * 1024u64.pow(3) - 1024 * 1024 {
            return Err("ZIP 超过 2 GiB，请拆分任务。".into());
        }
        files.push(part);
    }
    let archive_path = temp.join("archive.part");
    let file = std::fs::File::create(&archive_path).map_err(|_| "无法创建 ZIP。")?;
    let mut writer = zip::ZipWriter::new(file);
    use tokio::io::AsyncReadExt;
    for (i, (item, path)) in items.iter().zip(files.iter()).enumerate() {
        writer
            .start_file(
                &item.name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .map_err(|_| "ZIP 文件条目无效。")?;
        let mut input = tokio::fs::File::open(path)
            .await
            .map_err(|_| "打包源文件不可用。")?;
        let mut buffer = vec![0; 128 * 1024];
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err("打包已暂停，文件进度已保留。".into());
            }
            let n = input
                .read(&mut buffer)
                .await
                .map_err(|_| "打包文件读取失败。")?;
            if n == 0 {
                break;
            }
            writer
                .write_all(&buffer[..n])
                .map_err(|_| "ZIP 写入失败。")?;
            tokio::task::yield_now().await;
        }
        let _ = state.patch_task(app, &task.id, |t| {
            t.progress = 90. + (i + 1) as f64 / items.len() as f64 * 9.
        });
    }
    writer.finish().map_err(|_| "ZIP 完成失败。")?;
    if cancel.load(Ordering::Relaxed) {
        return Err("打包已暂停。".into());
    }
    let output = directory.join(format!(
        "{}-{}.zip",
        safe_filename(&task.title),
        &task.id[..8]
    ));
    tokio::fs::rename(&archive_path, &output)
        .await
        .map_err(|_| "ZIP 保存失败。")?;
    for file in files {
        crate::transfer::cleanup(&file).await;
        let _ = tokio::fs::remove_file(file).await;
    }
    let _ = tokio::fs::remove_dir(&temp).await;
    state.patch_task(app, &task.id, |t| {
        t.status = TaskStatus::Completed;
        t.progress = 100.;
        t.speed = 0;
        t.downloaded_bytes = completed;
        t.total_bytes = completed;
        t.output_path = Some(output.to_string_lossy().into_owned());
        t.error = None;
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn archive_rejects_paths_duplicates_and_private_urls() {
        let item = |url: &str, name: &str| ArchiveResource {
            url: url.into(),
            name: name.into(),
        };
        assert!(validate_items(&[item("https://example.com/a.jpg", "图片.jpg")]).is_ok());
        for name in ["../a", "..", "a/b", "a\\b", ""] {
            assert!(validate_items(&[item("https://example.com/a.jpg", name)]).is_err());
        }
        assert!(validate_items(&[item("https://127.0.0.1/a", "a")]).is_err());
        assert!(validate_items(&[
            item("https://example.com/a", "a"),
            item("https://example.com/b", "A")
        ])
        .is_err());
        assert!(validate_items(&[item("https://example.com/a.m3u8", "a")]).is_err());
    }
}
