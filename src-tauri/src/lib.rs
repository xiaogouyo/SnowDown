mod auth;
mod bilibili;
mod clipboard;
mod engine;
mod model;
mod site_sessions;
mod sites;
mod storage;
mod web_capture;

use model::*;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use storage::Storage;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, State,
};

pub struct AppState {
    tasks: Mutex<Vec<DownloadTask>>,
    settings: Mutex<Settings>,
    auth: auth::Auth,
    site_sessions: site_sessions::Sessions,
    active: Mutex<HashMap<String, Arc<AtomicBool>>>,
    storage: Storage,
    ffmpeg: Option<PathBuf>,
}

impl AppState {
    fn snapshot(&self) -> AppSnapshot {
        let mut tasks = self.tasks.lock().unwrap().clone();
        tasks.sort_by_key(|t| std::cmp::Reverse(t.created_at));
        let auth = self.auth.snapshot();
        AppSnapshot {
            tasks,
            settings: self.settings.lock().unwrap().clone(),
            media_ready: self.ffmpeg.is_some(),
            authenticated: matches!(
                auth.state.as_str(),
                "checking" | "authenticated" | "offline"
            ),
            auth,
            site_sessions: self.site_sessions.snapshot(),
            version: env!("CARGO_PKG_VERSION").into(),
        }
    }
    fn emit(&self, app: &AppHandle) {
        let _ = app.emit("snowdown://state", self.snapshot());
    }
    fn patch_task<F: FnOnce(&mut DownloadTask)>(
        &self,
        app: &AppHandle,
        id: &str,
        update: F,
    ) -> Result<(), String> {
        {
            let mut tasks = self.tasks.lock().unwrap();
            let task = tasks.iter_mut().find(|t| t.id == id).ok_or("任务不存在")?;
            update(task);
            self.storage.save_task(task)?;
        }
        self.emit(app);
        Ok(())
    }
}

#[tauri::command]
fn get_snapshot(state: State<'_, Arc<AppState>>) -> AppSnapshot {
    state.snapshot()
}

#[tauri::command]
async fn inspect_video(
    app: AppHandle,
    input: String,
    state: State<'_, Arc<AppState>>,
) -> Result<VideoInfo, String> {
    let source = sites::recognize(&input)?;
    if source.platform == Platform::Bilibili {
        bilibili::inspect(&source.url, &state.auth.cookie()).await
    } else {
        Ok(state.site_sessions.resolve(&app, &source, 0).await?.info)
    }
}

#[tauri::command]
fn read_clipboard_link(window: tauri::WebviewWindow) -> Result<Option<sites::Source>, String> {
    if !window.is_focused().unwrap_or(false) {
        return Ok(None);
    }
    Ok(clipboard::read_text()?.and_then(|text| sites::recognize(&text).ok()))
}

#[tauri::command]
async fn open_site_session(
    app: AppHandle,
    platform: Platform,
    input: Option<String>,
) -> Result<(), String> {
    site_sessions::open_official(&app, platform, input)
}

#[tauri::command]
async fn capture_site_session(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    platform: Platform,
) -> Result<(), String> {
    let cookie = site_sessions::capture(&app, platform).await?;
    state.site_sessions.save(platform, cookie)?;
    state.emit(&app);
    Ok(())
}

#[tauri::command]
fn clear_site_session(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    platform: Platform,
) -> Result<(), String> {
    state.site_sessions.clear(platform)?;
    state.emit(&app);
    Ok(())
}

#[tauri::command]
fn create_tasks(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    request: CreateTaskRequest,
) -> Result<(), String> {
    if state.ffmpeg.is_none() {
        return Err("未找到 FFmpeg，暂时不能开始下载。请安装完整客户端。".into());
    }
    if request.video.platform == Platform::Bilibili {
        bilibili::normalize_input(&request.video.bvid)?;
    } else {
        let source = sites::recognize(&request.video.source_url)?;
        if source.platform != request.video.platform {
            return Err("视频与来源平台不一致。".into());
        }
        if sites::video_id(&source)? != request.video.bvid {
            return Err("视频标识与来源不一致。".into());
        }
    }
    if !request
        .video
        .qualities
        .iter()
        .any(|q| q.id == request.quality)
    {
        return Err("请选择视频可用的画质。".into());
    }
    if request.page_cids.is_empty() {
        return Err("请至少选择一个分P。".into());
    }
    let settings = state.settings.lock().unwrap().clone();
    std::fs::create_dir_all(&settings.download_dir).map_err(|e| format!("下载目录不可用：{e}"))?;
    let mut tasks = state.tasks.lock().unwrap();
    let mut added = 0;
    let unique: HashSet<u64> = request.page_cids.into_iter().collect();
    if unique
        .iter()
        .any(|cid| !request.video.pages.iter().any(|page| page.cid == *cid))
    {
        return Err("分P不属于当前视频".into());
    }
    for page in request
        .video
        .pages
        .iter()
        .filter(|page| unique.contains(&page.cid))
    {
        let cid = page.cid;
        if tasks.iter().any(|t| {
            t.platform == request.video.platform
                && t.bvid == request.video.bvid
                && t.cid == cid
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
            continue;
        }
        let task = DownloadTask {
            platform: request.video.platform,
            source_url: request.video.source_url.clone(),
            id: uuid::Uuid::new_v4().to_string(),
            bvid: request.video.bvid.clone(),
            cid,
            title: request.video.title.clone(),
            page_title: page.part.clone(),
            page: page.page,
            cover: request.video.cover.clone(),
            quality: request.quality,
            quality_label: request
                .video
                .qualities
                .iter()
                .find(|q| q.id == request.quality)
                .map(|q| q.label.clone())
                .unwrap_or_else(|| "原始画质".into()),
            format: request.format.clone(),
            status: TaskStatus::Queued,
            progress: 0.0,
            downloaded_bytes: 0,
            total_bytes: 0,
            speed: 0,
            error: None,
            output_path: None,
            created_at: now_ms() + added,
            download_dir: settings.download_dir.clone(),
            save_cover: settings.save_cover,
            save_danmaku: request.video.platform == Platform::Bilibili && settings.save_danmaku,
            save_subtitles: request.video.platform == Platform::Bilibili && settings.save_subtitles,
            warning: None,
        };
        state.storage.save_task(&task)?;
        tasks.push(task);
        added += 1;
    }
    drop(tasks);
    state.emit(&app);
    if added == 0 {
        return Err("所选分P已有相同的下载任务。".into());
    }
    Ok(())
}

#[tauri::command]
fn task_action(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    id: String,
    action: String,
) -> Result<(), String> {
    if action == "reveal" {
        use tauri_plugin_opener::OpenerExt;
        let tasks = state.tasks.lock().unwrap();
        let task = tasks.iter().find(|t| t.id == id).ok_or("任务不存在")?;
        let path = task.output_path.as_ref().ok_or("任务还没有生成文件")?;
        return app
            .opener()
            .reveal_item_in_dir(path)
            .map_err(|e| e.to_string());
    }
    if action == "remove" {
        let mut tasks = state.tasks.lock().unwrap();
        let task = tasks.iter().find(|t| t.id == id).ok_or("任务不存在")?;
        if matches!(
            task.status,
            TaskStatus::Downloading | TaskStatus::Merging | TaskStatus::Queued
        ) || state.active.lock().unwrap().contains_key(&id)
        {
            return Err("请先暂停或取消任务，再移除记录。".into());
        }
        state.storage.remove_task(&id)?;
        tasks.retain(|t| t.id != id);
        drop(tasks);
        state.emit(&app);
        return Ok(());
    }
    state.patch_task(&app, &id, |task| match action.as_str() {
        "pause" if matches!(task.status, TaskStatus::Queued | TaskStatus::Downloading) => {
            if let Some(cancel) = state.active.lock().unwrap().get(&id) {
                cancel.store(true, Ordering::Relaxed);
            }
            task.status = TaskStatus::Paused;
            task.speed = 0;
        }
        "cancel" if !matches!(task.status, TaskStatus::Completed | TaskStatus::Cancelled) => {
            if let Some(cancel) = state.active.lock().unwrap().get(&id) {
                cancel.store(true, Ordering::Relaxed);
            }
            task.status = TaskStatus::Cancelled;
            task.speed = 0;
        }
        "resume"
            if matches!(
                task.status,
                TaskStatus::Paused | TaskStatus::Failed | TaskStatus::Cancelled
            ) =>
        {
            task.status = TaskStatus::Queued;
            task.error = None;
            task.warning = None;
        }
        _ => {}
    })
}

#[tauri::command]
fn save_settings(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    settings: Settings,
) -> Result<(), String> {
    if settings.download_dir.trim().is_empty() {
        return Err("请选择下载目录。".into());
    }
    if !PathBuf::from(&settings.download_dir).is_absolute() {
        return Err("下载目录必须是绝对路径。".into());
    }
    if !(1..=4).contains(&settings.concurrency) || settings.max_speed_mbps > 1024 {
        return Err("并发数应为1至4，单任务限速应在0至1024 MB/s之间。".into());
    }
    std::fs::create_dir_all(&settings.download_dir).map_err(|e| format!("无法使用此目录：{e}"))?;
    state.storage.save_settings(&settings)?;
    *state.settings.lock().unwrap() = settings;
    state.emit(&app);
    Ok(())
}

#[tauri::command]
async fn begin_login(state: State<'_, Arc<AppState>>) -> Result<auth::LoginQr, String> {
    state.auth.begin().await
}

#[tauri::command]
async fn poll_login(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<auth::LoginPoll, String> {
    let result = state.auth.poll(&id).await?;
    if result.state == "confirmed" {
        state.emit(&app);
    }
    Ok(result)
}

#[tauri::command]
fn cancel_login(state: State<'_, Arc<AppState>>) {
    state.auth.cancel();
}

#[tauri::command]
fn logout(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    state.auth.logout()?;
    state.emit(&app);
    Ok(())
}

#[tauri::command]
async fn check_login(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    state.auth.validate().await;
    state.emit(&app);
    Ok(())
}

#[tauri::command]
fn open_download_directory(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let directory = state.settings.lock().unwrap().download_dir.clone();
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    app.opener()
        .open_path(directory, None::<&str>)
        .map_err(|e| e.to_string())
}

fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn find_ffmpeg(app: &AppHandle) -> Option<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(resource) = app.path().resource_dir() {
        paths.push(resource.join("resources/ffmpeg.exe"));
        paths.push(resource.join("ffmpeg.exe"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            paths.push(parent.join("resources/ffmpeg.exe"));
            paths.push(parent.join("ffmpeg.exe"));
        }
    }
    if cfg!(debug_assertions) {
        paths.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/ffmpeg.exe"));
    }
    paths.into_iter().find(|p| p.is_file())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            show_main(app)
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let data_dir = handle.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let storage =
                Storage::open(&data_dir.join("snowdown.db")).map_err(std::io::Error::other)?;
            let default_download = handle
                .path()
                .download_dir()
                .unwrap_or_else(|_| data_dir.clone())
                .join("SnowDown");
            let settings = storage
                .load_settings(Settings {
                    download_dir: default_download.to_string_lossy().to_string(),
                    concurrency: 2,
                    max_speed_mbps: 0,
                    save_cover: false,
                    save_danmaku: false,
                    save_subtitles: false,
                    clipboard_enabled: true,
                    clipboard_auto_parse: false,
                })
                .map_err(std::io::Error::other)?;
            let tasks = storage.load_tasks().map_err(std::io::Error::other)?;
            let state = Arc::new(AppState {
                tasks: Mutex::new(tasks),
                settings: Mutex::new(settings),
                auth: auth::Auth::open(data_dir.join("bilibili-session.dat")),
                site_sessions: site_sessions::Sessions::open(data_dir.join("site-sessions")),
                active: Mutex::new(HashMap::new()),
                storage,
                ffmpeg: find_ffmpeg(&handle),
            });
            app.manage(state.clone());
            #[cfg(debug_assertions)]
            site_sessions::start_debug_probe(handle.clone());
            let show = MenuItem::with_id(app, "show", "打开 SnowDown", true, None::<&str>)?;
            let folder = MenuItem::with_id(app, "folder", "打开下载目录", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出 SnowDown", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &folder, &quit])?;
            let mut tray = TrayIconBuilder::with_id("main-tray")
                .tooltip("SnowDown · 好内容，慢慢收藏")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => show_main(app),
                    "folder" => {
                        let state = app.state::<Arc<AppState>>();
                        let _ = open_download_directory(app.clone(), state);
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if matches!(
                        event,
                        TrayIconEvent::DoubleClick {
                            button: MouseButton::Left,
                            ..
                        }
                    ) {
                        show_main(tray.app_handle());
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;
            let auth_handle = handle.clone();
            let auth_state = state.clone();
            tauri::async_runtime::spawn(async move {
                auth_state.auth.validate().await;
                auth_state.emit(&auth_handle);
            });
            engine::start_scheduler(handle, state);
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                } else if let Some(platform) = match window.label() {
                    "official-douyin" => Some(Platform::Douyin),
                    "official-xiaohongshu" => Some(Platform::Xiaohongshu),
                    _ => None,
                } {
                    api.prevent_close();
                    let handle = window.app_handle().clone();
                    let input = handle.get_webview_window(window.label()).and_then(|page| page.url().ok()).and_then(|url| sites::recognize(url.as_str()).ok()).map(|source| source.url);
                    if let Some(page) = handle.get_webview_window(window.label()) {
                        let _ = page.eval("document.querySelectorAll('video,audio').forEach(media => media.pause())");
                    }
                    let _ = window.hide();
                    show_main(&handle);
                    let state = handle.state::<Arc<AppState>>().inner().clone();
                    tauri::async_runtime::spawn(async move {
                        if let Ok(cookie) = site_sessions::capture(&handle, platform).await {
                            let _ = state.site_sessions.save(platform, cookie);
                        }
                        state.emit(&handle);
                        let _ = handle.emit("snowdown://site-return", serde_json::json!({"platform":platform,"input":input}));
                    });
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            inspect_video,
            create_tasks,
            task_action,
            save_settings,
            begin_login,
            poll_login,
            cancel_login,
            logout,
            check_login,
            read_clipboard_link,
            open_site_session,
            capture_site_session,
            clear_site_session,
            open_download_directory
        ])
        .run(tauri::generate_context!())
        .expect("SnowDown could not start");
}
