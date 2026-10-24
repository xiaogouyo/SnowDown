use crate::{bilibili, model::*, sites, AppState};
use std::{collections::HashSet, path::Path};

pub fn enqueue(
    state: &AppState,
    requests: Vec<CreateTaskRequest>,
    options: BatchOptions,
    batched: bool,
) -> Result<BatchResult, String> {
    if state.ffmpeg.is_none() {
        return Err("未找到 FFmpeg，暂时不能开始下载。请安装完整客户端。".into());
    }
    let mut tasks = state.tasks.lock().unwrap();
    if state.updater.installing() {
        return Err("正在退出安装更新，请完成更新后再添加下载。".into());
    }
    let batch_id = batched.then(|| uuid::Uuid::new_v4().to_string());
    let (added, skipped) = prepare(requests, &options, &tasks, batch_id, now_ms())?;
    if !added.is_empty() {
        std::fs::create_dir_all(&options.download_dir)
            .map_err(|e| format!("下载目录不可用：{e}"))?;
        state.storage.save_tasks(&added)?;
    }
    let count = added.len();
    tasks.extend(added);
    Ok(BatchResult {
        added: count,
        skipped,
    })
}

fn prepare(
    requests: Vec<CreateTaskRequest>,
    options: &BatchOptions,
    existing: &[DownloadTask],
    batch_id: Option<String>,
    now: u64,
) -> Result<(Vec<DownloadTask>, usize), String> {
    if requests.is_empty() || requests.len() > 100 {
        return Err("每批应包含 1 至 100 个视频链接。".into());
    }
    if options.download_dir.trim().is_empty() || !Path::new(&options.download_dir).is_absolute() {
        return Err("请选择有效的绝对路径作为下载目录。".into());
    }
    if options.interval_secs > 3600 {
        return Err("任务启动间隔应为 0 至 3600 秒。".into());
    }
    if requests.iter().map(|r| r.page_cids.len()).sum::<usize>() > 1000 {
        return Err("每批最多创建 1000 个分P任务，请分批下载。".into());
    }
    let mut added = Vec::<DownloadTask>::new();
    let mut skipped = 0;
    for request in requests {
        if request.video.platform == Platform::Bilibili {
            bilibili::normalize_input(&request.video.bvid)?;
        } else {
            let source = sites::recognize(&request.video.source_url)?;
            if source.platform != request.video.platform
                || sites::video_id(&source)? != request.video.bvid
            {
                return Err("视频标识与来源平台不一致。".into());
            }
        }
        let quality = request
            .video
            .qualities
            .iter()
            .find(|q| q.id == request.quality)
            .ok_or("请选择视频可用的画质。")?;
        let unique: HashSet<u64> = request.page_cids.into_iter().collect();
        if unique.is_empty()
            || unique
                .iter()
                .any(|cid| !request.video.pages.iter().any(|page| page.cid == *cid))
        {
            return Err("请选择属于当前视频的分P。".into());
        }
        for page in request
            .video
            .pages
            .iter()
            .filter(|page| unique.contains(&page.cid))
        {
            if existing.iter().chain(added.iter()).any(|t| {
                t.platform == request.video.platform
                    && t.bvid == request.video.bvid
                    && t.cid == page.cid
                    && t.format == request.format
                    && t.quality == request.quality
                    && matches!(
                        t.status,
                        TaskStatus::Queued
                            | TaskStatus::Downloading
                            | TaskStatus::Merging
                            | TaskStatus::Paused
                    )
            }) {
                skipped += 1;
                continue;
            }
            added.push(DownloadTask {
                platform: request.video.platform,
                source_url: request.video.source_url.clone(),
                id: uuid::Uuid::new_v4().to_string(),
                bvid: request.video.bvid.clone(),
                cid: page.cid,
                title: request.video.title.clone(),
                page_title: page.part.clone(),
                page: page.page,
                cover: request.video.cover.clone(),
                quality: quality.id,
                quality_label: quality.label.clone(),
                format: request.format.clone(),
                status: TaskStatus::Queued,
                progress: 0.0,
                downloaded_bytes: 0,
                total_bytes: 0,
                speed: 0,
                error: None,
                output_path: None,
                created_at: now + added.len() as u64,
                download_dir: options.download_dir.clone(),
                save_cover: options.save_cover,
                save_danmaku: request.video.platform == Platform::Bilibili && options.save_danmaku,
                save_subtitles: request.video.platform == Platform::Bilibili
                    && options.save_subtitles,
                warning: None,
                batch_id: batch_id.clone(),
                start_interval_secs: options.interval_secs,
                not_before: 0,
            });
        }
    }
    Ok((added, skipped))
}

/// Reserve a real start and postpone siblings from that instant, rather than from enqueue time.
/// Deadlines live on remaining tasks so removing the started record cannot remove the gap.
pub fn start_updates(tasks: &[DownloadTask], id: &str, now: u64) -> Option<Vec<DownloadTask>> {
    let task = tasks.iter().find(|task| task.id == id)?;
    if task.status != TaskStatus::Queued || task.not_before > now {
        return None;
    }
    let mut started = task.clone();
    started.status = TaskStatus::Downloading;
    started.error = None;
    started.not_before = 0;
    let mut updates = vec![started];
    if let Some(batch_id) = &task.batch_id {
        if task.start_interval_secs > 0 {
            let deadline = now.saturating_add(task.start_interval_secs as u64 * 1000);
            updates.extend(
                tasks
                    .iter()
                    .filter(|other| {
                        other.id != id
                            && other.batch_id.as_ref() == Some(batch_id)
                            && other.status != TaskStatus::Completed
                    })
                    .map(|other| {
                        let mut pending = other.clone();
                        pending.not_before = pending.not_before.max(deadline);
                        pending
                    }),
            );
        }
    }
    Some(updates)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::Storage;

    fn options() -> BatchOptions {
        BatchOptions {
            download_dir: std::env::temp_dir().to_string_lossy().into_owned(),
            interval_secs: 3,
            save_cover: true,
            save_danmaku: true,
            save_subtitles: true,
        }
    }
    fn request(id: &str, cids: &[u64]) -> CreateTaskRequest {
        CreateTaskRequest {
            video: VideoInfo {
                platform: Platform::Bilibili,
                source_url: format!("https://www.bilibili.com/video/{id}"),
                bvid: id.into(),
                title: "批量测试".into(),
                cover: "".into(),
                author: "".into(),
                duration: 1,
                pages: (1..=3)
                    .map(|cid| VideoPage {
                        cid,
                        page: cid as u32,
                        part: format!("P{cid}"),
                        duration: 1,
                    })
                    .collect(),
                qualities: vec![Quality {
                    id: 32,
                    label: "480P".into(),
                    width: 852,
                    height: 480,
                }],
                selected_cid: 1,
            },
            page_cids: cids.to_vec(),
            quality: 32,
            format: OutputFormat::Video,
        }
    }
    fn apply(tasks: &mut [DownloadTask], updates: Vec<DownloadTask>) {
        for update in updates {
            let current = tasks.iter_mut().find(|task| task.id == update.id).unwrap();
            *current = update;
        }
    }

    #[test]
    fn batch_deduplicates_pages_and_existing_tasks_without_changing_options() {
        let (first, _) = prepare(
            vec![request("BV1xx411c7mD", &[1])],
            &options(),
            &[],
            None,
            0,
        )
        .unwrap();
        let (tasks, skipped) = prepare(
            vec![
                request("BV1xx411c7mD", &[1, 2, 2]),
                request("BV1xx411c7mD", &[2, 3]),
            ],
            &options(),
            &first,
            Some("batch".into()),
            10,
        )
        .unwrap();
        assert_eq!(skipped, 2);
        assert_eq!(tasks.iter().map(|t| t.cid).collect::<Vec<_>>(), [2, 3]);
        assert!(tasks.iter().all(|t| t.batch_id.as_deref() == Some("batch")
            && t.save_cover
            && t.save_danmaku
            && t.save_subtitles
            && t.start_interval_secs == 3));
    }
    #[test]
    fn invalid_item_rejects_whole_batch_and_limits_are_enforced() {
        let mut invalid = request("BV1xx411c7mD", &[1]);
        invalid.quality = 999;
        assert!(prepare(
            vec![request("BV1xx411c7mD", &[1]), invalid],
            &options(),
            &[],
            None,
            0
        )
        .is_err());
        assert!(prepare(
            vec![request("BV1xx411c7mD", &[99])],
            &options(),
            &[],
            None,
            0
        )
        .is_err());
        let mut invalid = options();
        invalid.interval_secs = 3601;
        assert!(prepare(vec![request("BV1xx411c7mD", &[1])], &invalid, &[], None, 0).is_err());
        invalid = options();
        invalid.download_dir = "relative".into();
        assert!(prepare(vec![request("BV1xx411c7mD", &[1])], &invalid, &[], None, 0).is_err());
        assert!(prepare(
            (0..101).map(|_| request("BV1xx411c7mD", &[1])).collect(),
            &options(),
            &[],
            None,
            0
        )
        .is_err());
    }
    #[test]
    fn actual_start_gaps_survive_restart_record_removal_and_late_concurrency_slots() {
        let store = Storage::open(Path::new(":memory:")).unwrap();
        let (mut tasks, _) = prepare(
            vec![request("BV1xx411c7mD", &[1, 2, 3])],
            &options(),
            &[],
            Some("batch".into()),
            0,
        )
        .unwrap();
        let first = tasks[0].id.clone();
        let second = tasks[1].id.clone();
        let third = tasks[2].id.clone();
        let updates = start_updates(&tasks, &first, 10_000).unwrap();
        apply(&mut tasks, updates);
        store.save_tasks(&tasks).unwrap();
        assert!(start_updates(&tasks, &second, 12_999).is_none());
        // A free concurrency slot arrives much later: subsequent starts must still be spaced.
        let updates = start_updates(&tasks, &second, 30_000).unwrap();
        apply(&mut tasks, updates);
        assert!(start_updates(&tasks, &third, 30_000).is_none());
        assert!(start_updates(&tasks, &third, 32_999).is_none());
        store.save_tasks(&tasks).unwrap();
        store.remove_task(&second).unwrap();
        let mut loaded = store.load_tasks().unwrap();
        let pending = loaded.iter_mut().find(|t| t.id == third).unwrap();
        pending.status = TaskStatus::Queued;
        assert!(start_updates(&loaded, &third, 32_999).is_none());
        assert!(start_updates(&loaded, &third, 33_000).is_some());
    }
    #[test]
    fn unrelated_batches_and_zero_interval_do_not_block_each_other() {
        let (mut tasks, _) = prepare(
            vec![request("BV1xx411c7mD", &[1, 2])],
            &options(),
            &[],
            Some("batch".into()),
            0,
        )
        .unwrap();
        let (independent, _) = prepare(
            vec![request("BV1xx411c7mD", &[3])],
            &options(),
            &[],
            None,
            0,
        )
        .unwrap();
        tasks.extend(independent);
        let updates = start_updates(&tasks, &tasks[0].id, 100).unwrap();
        apply(&mut tasks, updates);
        assert!(start_updates(&tasks, &tasks[2].id, 100).is_some());
        tasks[1].status = TaskStatus::Paused;
        assert!(start_updates(&tasks, &tasks[1].id, 4000).is_none());
        let mut no_gap = options();
        no_gap.interval_secs = 0;
        let (mut tasks, _) = prepare(
            vec![request("BV1xx411c7mD", &[1, 2])],
            &no_gap,
            &[],
            Some("zero".into()),
            0,
        )
        .unwrap();
        let updates = start_updates(&tasks, &tasks[0].id, 100).unwrap();
        apply(&mut tasks, updates);
        assert!(start_updates(&tasks, &tasks[1].id, 100).is_some());
    }
    #[test]
    fn old_task_json_loads_without_batch_metadata_and_new_metadata_roundtrips() {
        let (tasks, _) = prepare(
            vec![request("BV1xx411c7mD", &[1])],
            &options(),
            &[],
            Some("batch".into()),
            0,
        )
        .unwrap();
        let mut json = serde_json::to_value(&tasks[0]).unwrap();
        for key in ["batchId", "startIntervalSecs", "notBefore"] {
            json.as_object_mut().unwrap().remove(key);
        }
        let legacy: DownloadTask = serde_json::from_value(json).unwrap();
        assert!(legacy.batch_id.is_none());
        assert_eq!(legacy.start_interval_secs, 0);
        assert_eq!(legacy.not_before, 0);
        let restored: DownloadTask =
            serde_json::from_str(&serde_json::to_string(&tasks[0]).unwrap()).unwrap();
        assert_eq!(restored.batch_id, tasks[0].batch_id);
    }
}
