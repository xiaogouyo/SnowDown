use crate::{bilibili, model::*, AppState};
use futures_util::StreamExt;
use reqwest::{header, Client, StatusCode};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tauri::AppHandle;
use tokio::{fs, io::AsyncWriteExt, process::Command};

pub fn start_scheduler(app: AppHandle, state: Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        loop {
            let limit = state.settings.lock().unwrap().concurrency;
            let queued: Vec<_> = state
                .tasks
                .lock()
                .unwrap()
                .iter()
                .filter(|t| t.status == TaskStatus::Queued)
                .cloned()
                .collect();
            for task in queued {
                let cancel = {
                    let mut active = state.active.lock().unwrap();
                    if active.len() >= limit {
                        break;
                    }
                    if active.contains_key(&task.id) {
                        continue;
                    }
                    let cancel = Arc::new(AtomicBool::new(false));
                    active.insert(task.id.clone(), cancel.clone());
                    cancel
                };
                let job_state = state.clone();
                let job_app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let id = task.id.clone();
                    let _ = job_state.patch_task(&job_app, &id, |t| {
                        if t.status == TaskStatus::Queued && !cancel.load(Ordering::Relaxed) {
                            t.status = TaskStatus::Downloading;
                            t.error = None;
                        }
                    });
                    let mut result = Err("下载没有启动".into());
                    for attempt in 0..3 {
                        if cancel.load(Ordering::Relaxed) {
                            break;
                        }
                        result = run_task(&job_app, &job_state, &task, &cancel).await;
                        if result.is_ok() || cancel.load(Ordering::Relaxed) {
                            break;
                        }
                        if attempt < 2 {
                            let message = format!("连接中断，正在进行第 {} 次重试…", attempt + 1);
                            let _ = job_state.patch_task(&job_app, &id, |t| {
                                t.error = Some(message);
                                t.speed = 0;
                            });
                            tokio::time::sleep(Duration::from_secs(2 << attempt)).await;
                        }
                    }
                    if let Err(error) = result {
                        if !cancel.load(Ordering::Relaxed) {
                            let _ = job_state.patch_task(&job_app, &id, |t| {
                                if !cancel.load(Ordering::Relaxed) {
                                    t.status = TaskStatus::Failed;
                                    t.error = Some(error);
                                    t.speed = 0;
                                }
                            });
                        }
                    }
                    job_state.active.lock().unwrap().remove(&id);
                    job_state.emit(&job_app);
                });
            }
            tokio::time::sleep(Duration::from_millis(350)).await;
        }
    });
}

fn check_cancel(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        Err("任务已暂停或取消".into())
    } else {
        Ok(())
    }
}

fn select_streams(data: &Value, quality: u32, audio_only: bool) -> Result<Vec<String>, String> {
    if let Some(videos) = data["dash"]["video"].as_array() {
        let mut streams = videos.iter().collect::<Vec<_>>();
        streams.sort_by_key(|v| {
            (
                std::cmp::Reverse(v["id"].as_u64().unwrap_or(0)),
                v["codecid"].as_u64().unwrap_or(0) != 7,
            )
        });
        let video = streams
            .iter()
            .find(|v| v["id"].as_u64().unwrap_or(0) <= quality as u64)
            .or(streams.first())
            .ok_or("没有可用的视频流")?;
        let audio = data["dash"]["audio"]
            .as_array()
            .and_then(|a| {
                a.iter()
                    .max_by_key(|v| v["bandwidth"].as_u64().unwrap_or(0))
            })
            .ok_or("没有取得音频流")?;
        let mut urls = vec![];
        if !audio_only {
            urls.push(bilibili::media_url(video)?);
        }
        urls.push(bilibili::media_url(audio)?);
        return Ok(urls);
    }
    let segments = data["durl"].as_array().ok_or("没有可用的音视频资源")?;
    segments.iter().map(bilibili::media_url).collect()
}

async fn run_task(
    app: &AppHandle,
    state: &Arc<AppState>,
    task: &DownloadTask,
    cancel: &Arc<AtomicBool>,
) -> Result<(), String> {
    check_cancel(cancel)?;
    let ffmpeg = state
        .ffmpeg
        .as_ref()
        .ok_or("未找到 FFmpeg。请在设置中检查组件，或重新安装完整客户端。")?;
    let cookie = state.auth.cookie();
    let api_client = bilibili::client(&cookie)?;
    let media_client = Client::builder()
        .user_agent(bilibili::USER_AGENT)
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(35))
        .build()
        .map_err(|e| e.to_string())?;
    let play = bilibili::play_info(&api_client, &task.bvid, task.cid, task.quality).await?;
    check_cancel(cancel)?;
    let actual_quality = bilibili::qualities(&play)
        .into_iter()
        .find(|q| q.id <= task.quality)
        .map(|q| q.id)
        .unwrap_or(task.quality);
    let urls = select_streams(&play, task.quality, task.format == OutputFormat::Audio)?;
    let is_dash = play["dash"].is_object();
    let directory = PathBuf::from(&task.download_dir);
    let temp_dir = directory.join(".snowdown").join(&task.id);
    fs::create_dir_all(&temp_dir)
        .await
        .map_err(|e| format!("无法创建下载目录：{e}"))?;
    let mut sizes = Vec::new();
    for url in &urls {
        check_cancel(cancel)?;
        let size = media_client
            .head(url)
            .header(header::REFERER, "https://www.bilibili.com/")
            .send()
            .await
            .ok()
            .and_then(|r| {
                if r.status().is_success() {
                    r.content_length()
                } else {
                    None
                }
            })
            .unwrap_or(0);
        sizes.push(size);
    }
    let total: u64 = sizes.iter().sum();
    let _ = state.patch_task(app, &task.id, |t| {
        if !cancel.load(Ordering::Relaxed) {
            t.quality_label = bilibili::quality_label(actual_quality).into();
            t.status = TaskStatus::Downloading;
            t.error = None;
            t.total_bytes = total;
        }
    });
    let mut paths = Vec::new();
    let mut offset = 0;
    for (index, url) in urls.iter().enumerate() {
        check_cancel(cancel)?;
        let file = temp_dir.join(format!("stream-{actual_quality}-{index}.part"));
        let speed_limit = state.settings.lock().unwrap().max_speed_mbps as u64 * 1024 * 1024;
        let downloaded = transfer(
            &media_client,
            url,
            &file,
            cancel,
            speed_limit,
            |received, expected, speed| {
                let progress = if total > 0 && sizes.iter().all(|n| *n > 0) {
                    ((offset + received) as f64 / total as f64 * 94.0).min(94.0)
                } else {
                    (index as f64
                        + if expected > 0 {
                            received as f64 / expected as f64
                        } else {
                            0.0
                        })
                        / urls.len() as f64
                        * 94.0
                };
                let _ = state.patch_task(app, &task.id, |t| {
                    if !cancel.load(Ordering::Relaxed) {
                        t.downloaded_bytes = offset + received;
                        t.progress = progress;
                        t.speed = speed;
                    }
                });
            },
        )
        .await?;
        offset += downloaded;
        paths.push(file);
    }
    check_cancel(cancel)?;
    let stem = format!(
        "{} [{}_P{}]-{}",
        safe_filename(&task.title),
        task.bvid,
        task.page,
        &task.id[..8]
    );
    let extension = if task.format == OutputFormat::Audio {
        "m4a"
    } else {
        "mp4"
    };
    let output = directory.join(format!("{stem}.{extension}"));
    let pending = temp_dir.join(format!("output.{extension}"));
    let _ = state.patch_task(app, &task.id, |t| {
        if !cancel.load(Ordering::Relaxed) {
            t.status = TaskStatus::Merging;
            t.progress = 95.0;
            t.speed = 0;
            t.total_bytes = offset;
            t.downloaded_bytes = offset;
        }
    });
    let mut args: Vec<String> = vec![
        "-nostdin".into(),
        "-y".into(),
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
    ];
    if is_dash {
        for path in &paths {
            args.extend(["-i".into(), path.to_string_lossy().to_string()]);
        }
        if task.format == OutputFormat::Audio {
            args.extend(["-vn".into(), "-c:a".into(), "copy".into()]);
        } else {
            args.extend([
                "-map".into(),
                "0:v:0".into(),
                "-map".into(),
                "1:a:0".into(),
                "-c".into(),
                "copy".into(),
            ]);
        }
    } else if paths.len() == 1 {
        args.extend(["-i".into(), paths[0].to_string_lossy().to_string()]);
        if task.format == OutputFormat::Audio {
            args.extend(["-vn".into(), "-c:a".into(), "aac".into()]);
        } else {
            args.extend(["-c".into(), "copy".into()]);
        }
    } else {
        let manifest = temp_dir.join("segments.txt");
        let entries = paths
            .iter()
            .map(|p| format!("file '{}'\n", p.file_name().unwrap().to_string_lossy()))
            .collect::<String>();
        fs::write(&manifest, entries)
            .await
            .map_err(|e| e.to_string())?;
        args.extend([
            "-f".into(),
            "concat".into(),
            "-safe".into(),
            "1".into(),
            "-i".into(),
            manifest.to_string_lossy().to_string(),
        ]);
        if task.format == OutputFormat::Audio {
            args.extend(["-vn".into(), "-c:a".into(), "aac".into()]);
        } else {
            args.extend(["-c".into(), "copy".into()]);
        }
    }
    args.extend([
        "-movflags".into(),
        "+faststart".into(),
        pending.to_string_lossy().to_string(),
    ]);
    run_ffmpeg(ffmpeg, &args, cancel).await?;
    check_cancel(cancel)?;
    fs::rename(&pending, &output)
        .await
        .map_err(|e| format!("保存视频失败：{e}"))?;
    state.patch_task(app, &task.id, |t| {
        t.status = TaskStatus::Completed;
        t.progress = 100.0;
        t.speed = 0;
        t.error = None;
        t.output_path = Some(output.to_string_lossy().to_string());
    })?;
    let mut warnings = vec![];
    if task.save_cover && !task.cover.is_empty() {
        let cover_path = directory.join(format!("{stem}.jpg"));
        match media_client.get(&task.cover).send().await {
            Ok(response) if response.status().is_success() => match response.bytes().await {
                Ok(body) => {
                    if let Err(e) = fs::write(cover_path, body).await {
                        warnings.push(format!("封面保存失败：{e}"));
                    }
                }
                Err(_) => warnings.push("封面获取失败".into()),
            },
            _ => warnings.push("封面获取失败".into()),
        }
    }
    if task.save_danmaku {
        let result = async {
            let response = media_client
                .get(format!("https://comment.bilibili.com/{}.xml", task.cid))
                .send()
                .await
                .map_err(|e| e.to_string())?
                .error_for_status()
                .map_err(|e| e.to_string())?;
            let body = response.bytes().await.map_err(|e| e.to_string())?;
            fs::write(directory.join(format!("{stem}.xml")), body)
                .await
                .map_err(|e| e.to_string())
        }
        .await;
        if result.is_err() {
            warnings.push("弹幕保存失败".into());
        }
    }
    if task.save_subtitles {
        match bilibili::subtitles(&api_client, &task.bvid, task.cid).await {
            Ok(items) => {
                if items.is_empty() {
                    warnings.push("此视频未提供可下载字幕".into());
                }
                for (language, body) in items {
                    if fs::write(
                        directory.join(format!("{stem}.{}.srt", safe_filename(&language))),
                        body,
                    )
                    .await
                    .is_err()
                    {
                        warnings.push("字幕保存失败".into());
                    }
                }
            }
            Err(_) => warnings.push("字幕获取失败，可检查登录状态后重试".into()),
        }
    }
    // Once the media file is safely committed, optional sidecar failures never discard it.
    state.patch_task(app, &task.id, |t| {
        t.warning = if warnings.is_empty() {
            None
        } else {
            Some(warnings.join("；"))
        };
    })?;
    for path in paths {
        let _ = fs::remove_file(&path).await;
        let _ = fs::remove_file(path.with_extension("etag")).await;
    }
    let _ = fs::remove_file(temp_dir.join("segments.txt")).await;
    let _ = fs::remove_dir(&temp_dir).await;
    Ok(())
}

async fn wait_cancel(cancel: &AtomicBool) {
    while !cancel.load(Ordering::Relaxed) {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

pub async fn run_ffmpeg(binary: &Path, args: &[String], cancel: &AtomicBool) -> Result<(), String> {
    let mut command = Command::new(binary);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let child = command
        .spawn()
        .map_err(|e| format!("无法启动音视频合并组件：{e}"))?;
    tokio::select! {
        result = child.wait_with_output() => {
            let output = result.map_err(|e| e.to_string())?;
            if output.status.success() { Ok(()) }
            else { Err(format!("音视频处理失败：{}", String::from_utf8_lossy(&output.stderr).chars().take(800).collect::<String>())) }
        }
        _ = wait_cancel(cancel) => Err("任务已取消".into()),
    }
}

pub async fn transfer<F: FnMut(u64, u64, u64)>(
    client: &Client,
    url: &str,
    path: &Path,
    cancel: &AtomicBool,
    max_bytes_per_second: u64,
    mut progress: F,
) -> Result<u64, String> {
    let mut offset = fs::metadata(path).await.map(|m| m.len()).unwrap_or(0);
    let mut request = client
        .get(url)
        .header(header::REFERER, "https://www.bilibili.com/");
    if offset > 0 {
        request = request.header(header::RANGE, format!("bytes={offset}-"));
        if let Ok(etag) = fs::read_to_string(path.with_extension("etag")).await {
            request = request.header(header::IF_RANGE, etag);
        }
    }
    let response = tokio::select! {
        result = request.send() => result.map_err(|e| format!("下载连接失败：{e}"))?,
        _ = wait_cancel(cancel) => return Err("任务已暂停或取消".into()),
    };
    let status = response.status();
    if status == StatusCode::RANGE_NOT_SATISFIABLE {
        let remote_total = response
            .headers()
            .get(header::CONTENT_RANGE)
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.strip_prefix("bytes */"))
            .and_then(|n| n.parse::<u64>().ok());
        if remote_total == Some(offset) && offset > 0 {
            progress(offset, offset, 0);
            return Ok(offset);
        }
        // A changed resource must be fetched anew, never append it to stale bytes.
        fs::write(path, []).await.map_err(|e| e.to_string())?;
        return Err("源文件已变化，正在重新获取完整文件。".into());
    }
    if !status.is_success() {
        return Err(format!(
            "下载源返回 HTTP {status}，可稍后重试或检查登录状态。"
        ));
    }
    let total;
    if status == StatusCode::PARTIAL_CONTENT {
        let range = response
            .headers()
            .get(header::CONTENT_RANGE)
            .and_then(|h| h.to_str().ok())
            .ok_or("下载源缺少断点校验信息")?;
        let (start, remote_total) =
            parse_content_range(range).ok_or("下载源返回了无效的断点范围")?;
        if start != offset {
            return Err("下载源的断点位置不一致，已停止以避免文件损坏。".into());
        }
        total = remote_total;
    } else {
        offset = 0;
        total = response.content_length().unwrap_or(0);
    }
    if let Some(etag) = response
        .headers()
        .get(header::ETAG)
        .and_then(|v| v.to_str().ok())
    {
        let _ = fs::write(path.with_extension("etag"), etag).await;
    }
    let mut options = fs::OpenOptions::new();
    options.create(true).write(true);
    if offset > 0 {
        options.append(true);
    } else {
        options.truncate(true);
    }
    let mut file = options
        .open(path)
        .await
        .map_err(|e| format!("无法写入文件：{e}"))?;
    let start = Instant::now();
    let mut sample_time = Instant::now();
    let mut sample_bytes = offset;
    let mut received = offset;
    let mut stream = response.bytes_stream();
    loop {
        let next = tokio::select! {
            item = stream.next() => item,
            _ = wait_cancel(cancel) => { file.flush().await.map_err(|e| e.to_string())?; return Err("任务已暂停或取消".into()); }
        };
        let Some(chunk) = next else {
            break;
        };
        check_cancel(cancel)?;
        let chunk = chunk.map_err(|e| format!("下载中断：{e}"))?;
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("磁盘写入失败：{e}"))?;
        received += chunk.len() as u64;
        if max_bytes_per_second > 0 {
            let expected =
                Duration::from_secs_f64((received - offset) as f64 / max_bytes_per_second as f64);
            if expected > start.elapsed() {
                tokio::select! { _ = tokio::time::sleep(expected - start.elapsed()) => {}, _ = wait_cancel(cancel) => return Err("任务已暂停或取消".into()) }
            }
        }
        let elapsed = sample_time.elapsed().as_secs_f64();
        if elapsed >= 0.35 {
            progress(
                received,
                total,
                ((received - sample_bytes) as f64 / elapsed) as u64,
            );
            sample_time = Instant::now();
            sample_bytes = received;
        }
    }
    file.flush().await.map_err(|e| e.to_string())?;
    if total > 0 && received != total {
        return Err(format!(
            "文件长度校验失败：应为 {total} 字节，实际为 {received} 字节。"
        ));
    }
    progress(received, total, 0);
    Ok(received)
}

fn parse_content_range(value: &str) -> Option<(u64, u64)> {
    let value = value.strip_prefix("bytes ")?;
    let (range, total) = value.split_once('/')?;
    let (start, end) = range.split_once('-')?;
    let (start, end, total) = (
        start.parse::<u64>().ok()?,
        end.parse::<u64>().ok()?,
        total.parse::<u64>().ok()?,
    );
    if start > end || end >= total {
        return None;
    }
    Some((start, total))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };
    #[test]
    fn ranges_are_strictly_validated() {
        assert_eq!(parse_content_range("bytes 5-9/10"), Some((5, 10)));
        assert_eq!(parse_content_range("bytes 9-5/10"), None);
        assert_eq!(parse_content_range("bytes 0-10/10"), None);
        assert_eq!(parse_content_range("bytes */10"), None);
    }
    async fn fixture(response: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0; 4096];
            let _ = socket.read(&mut buffer).await;
            socket.write_all(response.as_bytes()).await.unwrap();
        });
        format!("http://{address}/file")
    }
    #[tokio::test]
    async fn ignored_range_restarts_instead_of_corrupting_file() {
        let path = std::env::temp_dir().join(format!("snowdown-{}.part", uuid::Uuid::new_v4()));
        fs::write(&path, b"old").await.unwrap();
        let url =
            fixture("HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello").await;
        transfer(
            &Client::new(),
            &url,
            &path,
            &AtomicBool::new(false),
            0,
            |_, _, _| {},
        )
        .await
        .unwrap();
        assert_eq!(fs::read(&path).await.unwrap(), b"hello");
        let _ = fs::remove_file(path).await;
    }
    #[tokio::test]
    async fn correct_partial_range_resumes() {
        let path = std::env::temp_dir().join(format!("snowdown-{}.part", uuid::Uuid::new_v4()));
        fs::write(&path, b"hel").await.unwrap();
        let url = fixture("HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 3-4/5\r\nContent-Length: 2\r\nConnection: close\r\n\r\nlo").await;
        transfer(
            &Client::new(),
            &url,
            &path,
            &AtomicBool::new(false),
            0,
            |_, _, _| {},
        )
        .await
        .unwrap();
        assert_eq!(fs::read(&path).await.unwrap(), b"hello");
        let _ = fs::remove_file(path).await;
    }
    #[tokio::test]
    async fn wrong_partial_range_leaves_existing_file_intact() {
        let path = std::env::temp_dir().join(format!("snowdown-{}.part", uuid::Uuid::new_v4()));
        fs::write(&path, b"hel").await.unwrap();
        let url = fixture("HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 0-1/5\r\nContent-Length: 2\r\nConnection: close\r\n\r\nlo").await;
        assert!(transfer(
            &Client::new(),
            &url,
            &path,
            &AtomicBool::new(false),
            0,
            |_, _, _| {}
        )
        .await
        .is_err());
        assert_eq!(fs::read(&path).await.unwrap(), b"hel");
        let _ = fs::remove_file(path).await;
    }

    #[tokio::test]
    async fn bundled_ffmpeg_produces_a_playable_audio_video_file() {
        let resources = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources");
        let binary = resources.join("ffmpeg.exe");
        if !binary.is_file() {
            eprintln!("Media smoke test requires npm run setup:media");
            return;
        }
        let output =
            std::env::temp_dir().join(format!("snowdown-media-{}.mp4", uuid::Uuid::new_v4()));
        let args = vec![
            "-nostdin",
            "-y",
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=0x3478ee:s=320x180:d=0.5",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=0.5",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-shortest",
        ]
        .into_iter()
        .map(str::to_string)
        .chain([output.to_string_lossy().to_string()])
        .collect::<Vec<_>>();
        run_ffmpeg(&binary, &args, &AtomicBool::new(false))
            .await
            .unwrap();
        let mut probe = Command::new(resources.join("ffprobe.exe"));
        probe
            .args([
                "-v",
                "error",
                "-show_entries",
                "stream=codec_type",
                "-of",
                "json",
            ])
            .arg(&output);
        #[cfg(windows)]
        probe.creation_flags(0x08000000);
        let result = probe.output().await.unwrap();
        let metadata: Value = serde_json::from_slice(&result.stdout).unwrap();
        let streams = metadata["streams"].as_array().unwrap();
        assert!(streams.iter().any(|s| s["codec_type"] == "video"));
        assert!(streams.iter().any(|s| s["codec_type"] == "audio"));
        let _ = fs::remove_file(output).await;
    }

    #[tokio::test]
    #[ignore = "Downloads a public Bilibili sample and verifies the full media pipeline"]
    async fn live_download_and_merge() {
        let api_client = bilibili::client("").unwrap();
        let info = bilibili::inspect("https://www.bilibili.com/video/BV17x411w7KC/?p=10", "")
            .await
            .unwrap();
        let play = bilibili::play_info(&api_client, &info.bvid, info.selected_cid, 16)
            .await
            .unwrap();
        let urls = select_streams(&play, 16, false).unwrap();
        assert_eq!(
            urls.len(),
            2,
            "Expected separate DASH video and audio streams"
        );
        let directory =
            std::env::temp_dir().join(format!("snowdown-live-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&directory).await.unwrap();
        let client = Client::builder()
            .user_agent(bilibili::USER_AGENT)
            .connect_timeout(Duration::from_secs(15))
            .read_timeout(Duration::from_secs(30))
            .build()
            .unwrap();
        let cancel = AtomicBool::new(false);
        let mut files = Vec::new();
        for (index, url) in urls.iter().enumerate() {
            let path = directory.join(format!("stream-{index}.part"));
            let size = transfer(&client, url, &path, &cancel, 0, |_, _, _| {})
                .await
                .unwrap();
            assert!(size > 0);
            eprintln!("Live download stream {index}: {size} bytes");
            files.push(path);
        }
        let resources = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources");
        let output = directory.join("merged.mp4");
        let args = vec![
            "-nostdin".into(),
            "-y".into(),
            "-hide_banner".into(),
            "-loglevel".into(),
            "error".into(),
            "-i".into(),
            files[0].to_string_lossy().to_string(),
            "-i".into(),
            files[1].to_string_lossy().to_string(),
            "-map".into(),
            "0:v:0".into(),
            "-map".into(),
            "1:a:0".into(),
            "-c".into(),
            "copy".into(),
            "-movflags".into(),
            "+faststart".into(),
            output.to_string_lossy().to_string(),
        ];
        run_ffmpeg(&resources.join("ffmpeg.exe"), &args, &cancel)
            .await
            .unwrap();
        let mut probe = Command::new(resources.join("ffprobe.exe"));
        probe
            .args([
                "-v",
                "error",
                "-show_entries",
                "stream=codec_type:format=duration",
                "-of",
                "json",
            ])
            .arg(&output);
        #[cfg(windows)]
        probe.creation_flags(0x08000000);
        let result = probe.output().await.unwrap();
        let metadata: Value = serde_json::from_slice(&result.stdout).unwrap();
        let streams = metadata["streams"].as_array().unwrap();
        assert!(streams.iter().any(|s| s["codec_type"] == "video"));
        assert!(streams.iter().any(|s| s["codec_type"] == "audio"));
        let duration = metadata["format"]["duration"]
            .as_str()
            .unwrap()
            .parse::<f64>()
            .unwrap();
        assert!(duration > 190.0);
        eprintln!("Live download and mux OK: {duration:.2}s, video + audio");
        for path in files {
            let _ = fs::remove_file(&path).await;
            let _ = fs::remove_file(path.with_extension("etag")).await;
        }
        let _ = fs::remove_file(output).await;
        let _ = fs::remove_dir(directory).await;
    }
}
