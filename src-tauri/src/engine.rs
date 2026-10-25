use crate::{bilibili, model::*, sites, AppState};
use reqwest::{header, Client};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tauri::AppHandle;
use tokio::{fs, process::Command};

pub fn start_scheduler(app: AppHandle, state: Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        loop {
            if state.stopping.load(Ordering::SeqCst) {
                break;
            }
            let limit = state.settings.lock().unwrap().concurrency;
            let mut queued: Vec<_> = state
                .tasks
                .lock()
                .unwrap()
                .iter()
                .filter(|t| t.status == TaskStatus::Queued)
                .cloned()
                .collect();
            queued.sort_by_key(|task| task.created_at);
            for task in queued {
                // Use the same tasks -> active lock order as pause/cancel/remove. Reserve the
                // status and persisted sibling deadlines before another task can be selected.
                let cancel = {
                    let mut tasks = state.tasks.lock().unwrap();
                    if state.updater.installing() || state.stopping.load(Ordering::SeqCst) {
                        break;
                    }
                    let mut active = state.active.lock().unwrap();
                    if active.len() >= limit {
                        break;
                    }
                    if active.contains_key(&task.id) {
                        continue;
                    }
                    let Some(updates) = crate::batch::start_updates(&tasks, &task.id, now_ms())
                    else {
                        continue;
                    };
                    if state.storage.save_tasks(&updates).is_err() {
                        continue;
                    }
                    for update in updates {
                        if let Some(current) = tasks.iter_mut().find(|t| t.id == update.id) {
                            *current = update;
                        }
                    }
                    let cancel = Arc::new(AtomicBool::new(false));
                    active.insert(task.id.clone(), cancel.clone());
                    cancel
                };
                state.emit(&app);
                state.notifications.show(&app, format!("start:{}", task.id), "开始下载", &task.title);
                let job_state = state.clone();
                let job_app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let id = task.id.clone();
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
                    // Serialize cleanup ownership with task removal; no locks cross await.
                    let removed = {
                        let tasks = job_state.tasks.lock().unwrap();
                        job_state.active.lock().unwrap().remove(&id);
                        !tasks.iter().any(|task| task.id == id)
                    };
                    if removed {
                        crate::task_files::cleanup(task.download_dir.clone(), id.clone()).await;
                    }
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

pub(crate) fn media_client(platform: Platform) -> Result<Client, String> {
    Client::builder()
        .http1_only()
        .pool_max_idle_per_host(16)
        .user_agent(bilibili::USER_AGENT)
        .default_headers({
            let mut headers = header::HeaderMap::new();
            headers.insert(
                header::REFERER,
                header::HeaderValue::from_static(platform.home()),
            );
            headers
        })
        .redirect(reqwest::redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() >= 10 {
                return attempt.error("媒体跳转次数过多");
            }
            if platform == Platform::Bilibili
                || sites::media_url(platform, attempt.url().as_str()).is_ok()
            {
                attempt.follow()
            } else {
                attempt.error("媒体跳转到非所属平台")
            }
        }))
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(35))
        .build()
        .map_err(|_| "无法初始化媒体下载组件。".into())
}

async fn run_task(
    app: &AppHandle,
    state: &Arc<AppState>,
    task: &DownloadTask,
    cancel: &Arc<AtomicBool>,
) -> Result<(), String> {
    check_cancel(cancel)?;
    if task.resource.is_some() {
        return crate::resource_download::run(app, state, task, cancel).await;
    }
    let ffmpeg = state
        .ffmpeg
        .as_ref()
        .ok_or("未找到 FFmpeg。请在设置中检查组件，或重新安装完整客户端。")?;
    let cookie = if task.platform == Platform::Bilibili {
        state.auth.cookie()
    } else {
        String::new()
    };
    let api_client = bilibili::client(&cookie)?;
    let media_client = media_client(task.platform)?;
    let (urls, is_dash, actual_quality, quality_label) = if task.platform == Platform::Bilibili {
        let play = bilibili::play_info(&api_client, &task.bvid, task.cid, task.quality).await?;
        check_cancel(cancel)?;
        let actual_quality = bilibili::qualities(&play)
            .into_iter()
            .find(|q| q.id <= task.quality)
            .map(|q| q.id)
            .unwrap_or(task.quality);
        (
            select_streams(&play, task.quality, task.format == OutputFormat::Audio)?,
            play["dash"].is_object(),
            actual_quality,
            bilibili::quality_label(actual_quality).to_string(),
        )
    } else {
        let source = sites::recognize(&task.source_url)?;
        if source.platform != task.platform {
            return Err("下载任务的平台与来源不一致。".into());
        }
        let resolved = state
            .site_sessions
            .resolve(app, &source, task.quality)
            .await?;
        check_cancel(cancel)?;
        let label = resolved
            .info
            .qualities
            .iter()
            .find(|q| q.id == resolved.quality)
            .map(|q| q.label.clone())
            .unwrap_or_else(|| "原始画质".into());
        let (urls, dash) = if let Some(audio) = resolved.audio_url {
            (
                if task.format == OutputFormat::Audio {
                    vec![audio]
                } else {
                    vec![resolved.url, audio]
                },
                true,
            )
        } else {
            (vec![resolved.url], false)
        };
        (urls, dash, resolved.quality, label)
    };
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
            t.quality_label = quality_label;
            t.status = TaskStatus::Downloading;
            t.error = None;
            t.total_bytes = total;
        }
    });
    let mut paths = Vec::new();
    let mut offset = 0;
    let mut resumed_offset = 0;
    for (index, url) in urls.iter().enumerate() {
        check_cancel(cancel)?;
        let file = temp_dir.join(format!("stream-{actual_quality}-{index}.part"));
        let settings = state.settings.lock().unwrap().clone();
        let identity = format!(
            "media:{}:{}:{}:{}:{index}",
            task.platform.key(),
            task.bvid,
            task.cid,
            actual_quality
        );
        let downloaded = crate::transfer::download(
            &media_client,
            url,
            &file,
            crate::transfer::Options {
                identity: &identity,
                expected_size: None,
                sha256: None,
                connections: settings.download_connections,
                max_bytes_per_second: settings.max_speed_mbps as u64 * 1024 * 1024,
            },
            cancel,
            |stats| {
                let (received, expected, speed) = (stats.downloaded, stats.total, stats.speed);
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
                        t.active_connections = stats.active_connections;
                        t.peak_connections = t.peak_connections.max(stats.active_connections);
                        t.resumed_bytes = resumed_offset + stats.resumed_bytes;
                        t.range_download = stats.ranged;
                    }
                });
            },
        )
        .await?;
        offset += downloaded.bytes;
        resumed_offset += downloaded.resumed_bytes;
        let _ = state.patch_task(app, &task.id, |t| {
            t.active_connections = 0;
            t.peak_connections = t.peak_connections.max(downloaded.peak_connections);
            t.range_download = downloaded.ranged;
        });
        paths.push(file);
    }
    check_cancel(cancel)?;
    let stem = format!(
        "{} [{}_P{}]-{}",
        safe_filename(&task.title),
        format!("{}_{}", task.platform.key(), task.bvid),
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
    if task.platform == Platform::Bilibili && task.save_danmaku {
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
    if task.platform == Platform::Bilibili && task.save_subtitles {
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
        crate::transfer::cleanup(&path).await;
        crate::transfer::cleanup(&path).await;
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

#[cfg(test)]
mod tests {
    use super::*;
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
        let client = media_client(Platform::Bilibili).unwrap();
        let cancel = AtomicBool::new(false);
        let mut files = Vec::new();
        for (index, url) in urls.iter().enumerate() {
            let path = directory.join(format!("stream-{index}.part"));
            let identity = format!("live-public-media-{index}");
            if index == 0 {
                let partial = crate::transfer::download(
                    &client,
                    url,
                    &path,
                    crate::transfer::Options {
                        identity: &identity,
                        expected_size: None,
                        sha256: None,
                        connections: 8,
                        max_bytes_per_second: 1024 * 1024,
                    },
                    &cancel,
                    |stats| {
                        if stats.downloaded >= 256 * 1024 {
                            cancel.store(true, Ordering::Relaxed)
                        }
                    },
                )
                .await;
                assert_eq!(partial.unwrap_err(), crate::transfer::CANCELLED);
                assert!(crate::transfer::saved_bytes(&path, &identity).await > 0);
                cancel.store(false, Ordering::Relaxed);
            }
            let outcome = crate::transfer::download(
                &media_client(Platform::Bilibili).unwrap(),
                url,
                &path,
                crate::transfer::Options {
                    identity: &identity,
                    expected_size: None,
                    sha256: None,
                    connections: 8,
                    max_bytes_per_second: 0,
                },
                &cancel,
                |_| {},
            )
            .await
            .unwrap();
            if index == 0 {
                assert!(outcome.resumed_bytes > 0);
                assert!(outcome.peak_connections > 1);
            }
            eprintln!(
                "Live stream {index}: ranged={}, peak={}, resumed={} bytes",
                outcome.ranged, outcome.peak_connections, outcome.resumed_bytes
            );
            let size = outcome.bytes;
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
