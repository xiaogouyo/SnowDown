mod auth;
mod batch;
#[cfg(debug_assertions)]
mod batch_probe;
mod bilibili;
mod browser_bridge;
mod clipboard;
#[cfg(debug_assertions)]
mod clipboard_probe;
mod clipboard_watch;
#[cfg(debug_assertions)]
mod debug_probe;
mod engine;
mod extension_package;
#[cfg(all(debug_assertions, windows))]
mod extension_package_probe;
mod extractor_proxy;
#[cfg(all(debug_assertions, windows))]
mod login_check_probe;
#[cfg(debug_assertions)]
mod minimal_probe;
#[cfg(all(debug_assertions, windows))]
mod mode_morph_probe;
mod model;
mod notifications;
mod qr_login;
mod resource_download;
mod site_sessions;
mod sites;
mod storage;
mod task_files;
mod transfer;
mod tray_menu;
mod update_cleanup;
mod update_mirrors;
mod update_notice;
mod updater;
mod web_capture;
mod web_extractor;

use model::*;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use storage::Storage;
use tauri::{
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, State,
};

pub struct AppState {
    tasks: Mutex<Vec<DownloadTask>>,
    settings: Mutex<Settings>,
    auth: auth::Auth,
    site_sessions: site_sessions::Sessions,
    qr_login: qr_login::QrLogin,
    active: Mutex<HashMap<String, Arc<AtomicBool>>>,
    storage: Storage,
    ffmpeg: Option<PathBuf>,
    updater: Arc<updater::Updater>,
    clipboard: clipboard_watch::Monitor,
    stopping: AtomicBool,
    update_notice: update_notice::Notice,
    browser: browser_bridge::Bridge,
    notifications: notifications::Notifications,
    extractor: web_extractor::Extractor,
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
        let mut completed = None;
        {
            let mut tasks = self.tasks.lock().unwrap();
            let task = tasks.iter_mut().find(|t| t.id == id).ok_or("任务不存在")?;
            let previous = task.status.clone();
            update(task);
            if task.status != TaskStatus::Downloading {
                task.active_connections = 0;
                task.speed = 0;
            }
            self.storage.save_task(task)?;
            if previous != TaskStatus::Completed && task.status == TaskStatus::Completed {
                completed = Some(task.page_title.clone());
            }
        }
        self.emit(app);
        if let Some(title) = completed {
            self.notifications
                .show(app, format!("complete:{id}"), "下载完成", &title);
        }
        Ok(())
    }
}

pub(crate) fn quit_after_checkpoint(app: &AppHandle) {
    let state = app.state::<Arc<AppState>>().inner().clone();
    if state.stopping.swap(true, Ordering::SeqCst) {
        return;
    }
    {
        let mut tasks = state.tasks.lock().unwrap();
        for cancel in state.active.lock().unwrap().values() {
            cancel.store(true, Ordering::Relaxed);
        }
        for task in tasks.iter_mut().filter(|task| {
            matches!(
                task.status,
                TaskStatus::Queued | TaskStatus::Downloading | TaskStatus::Merging
            )
        }) {
            task.status = TaskStatus::Paused;
            task.speed = 0;
            task.active_connections = 0;
            let _ = state.storage.save_task(task);
        }
    }
    state.updater.cancel();
    let app = app.clone();
    state.emit(&app);
    tauri::async_runtime::spawn(async move {
        for _ in 0..80 {
            if state.active.lock().unwrap().is_empty()
                && !matches!(
                    state.updater.status().phase,
                    updater::Phase::Downloading | updater::Phase::Probing
                )
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        app.exit(0);
    });
}

#[tauri::command]
fn get_snapshot(state: State<'_, Arc<AppState>>) -> AppSnapshot {
    state.snapshot()
}

#[tauri::command]
fn update_status(state: State<'_, Arc<AppState>>) -> updater::UpdateStatus {
    state.updater.status()
}

#[tauri::command]
async fn check_update(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<updater::UpdateStatus, String> {
    let status = state.updater.check().await?;
    update_notice::sync(&app, &status);
    Ok(status)
}

#[tauri::command]
fn dismiss_update_notice(app: AppHandle) {
    update_notice::dismiss(&app);
}
#[tauri::command]
fn browser_status(state: State<'_, Arc<AppState>>) -> Vec<browser_bridge::BrowserStatus> {
    browser_bridge::snapshot(&state)
}
#[tauri::command]
fn install_browser_extension(app: AppHandle, browser: String) -> Result<String, String> {
    browser_bridge::install(&app, &browser)
}
#[tauri::command]
async fn export_browser_extension_zip(app: AppHandle, path: String) -> Result<String, String> {
    let source = app
        .path()
        .resource_dir()
        .map_err(|_| "插件资源不可用。")?
        .join("resources/browser-extension");
    tauri::async_runtime::spawn_blocking(move || {
        extension_package::export(&source, std::path::Path::new(&path)).map(|p| {
            let _ = browser_bridge::prepare_zip_bindings();
            p.to_string_lossy().into_owned()
        })
    })
    .await
    .map_err(|_| "导出插件 ZIP 失败。".to_string())?
}

pub fn native_browser_host() -> bool {
    browser_bridge::native_host()
}

#[tauri::command]
fn defer_update(state: State<'_, Arc<AppState>>) -> Result<updater::UpdateStatus, String> {
    state.updater.defer()
}

#[tauri::command]
fn cancel_update_download(state: State<'_, Arc<AppState>>) {
    state.updater.cancel();
}

#[tauri::command]
fn download_update(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<updater::UpdateStatus, String> {
    let settings = state.settings.lock().unwrap().clone();
    state.updater.clone().start_download(
        settings.download_connections,
        settings.max_speed_mbps as u64 * 1024 * 1024,
        move |status| {
            update_notice::sync(&app, &status);
            let _ = app.emit("snowdown://update", status);
        },
    )
}

#[tauri::command]
fn open_update_release(app: AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(updater::releases_url(), None::<&str>)
        .map_err(|_| "无法打开 GitHub 发布页面。".into())
}

#[tauri::command]
async fn install_update(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let _operation = state
        .updater
        .operation
        .try_lock()
        .map_err(|_| "更新操作正在进行。")?;
    let _installer = match state.updater.verified_installer().await {
        Ok(path) => path,
        Err(error) => {
            let _ = app.emit("snowdown://update", state.updater.status());
            return Err(error);
        }
    };
    let current = std::env::current_exe().map_err(|_| "无法确定当前软件目录。")?;
    let directory = current.parent().ok_or("无法确定当前软件目录。")?;
    let path_text = directory.to_string_lossy();
    if directory.parent().is_none()
        || path_text.starts_with("\\\\")
        || path_text.encode_utf16().count() > 180
    {
        return Err("当前目录不适合自动覆盖安装，请下载更新并选择独立的软件文件夹。".into());
    }
    state.updater.mark_installing();
    let result = async {
        {
            let mut tasks = state.tasks.lock().unwrap();
            for cancel in state.active.lock().unwrap().values() {
                cancel.store(true, Ordering::Relaxed);
            }
            let mut updates = Vec::new();
            for task in tasks.iter_mut().filter(|task| {
                matches!(
                    task.status,
                    TaskStatus::Queued | TaskStatus::Downloading | TaskStatus::Merging
                )
            }) {
                task.status = TaskStatus::Paused;
                task.speed = 0;
                task.active_connections = 0;
                task.error = None;
                updates.push(task.clone());
            }
            state.storage.save_tasks(&updates)?;
        }
        state.emit(&app);
        let _ = app.emit("snowdown://update", state.updater.status());
        for _ in 0..600 {
            if state.active.lock().unwrap().is_empty() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        if !state.active.lock().unwrap().is_empty() {
            return Err("下载任务仍在保存进度，请稍后再次安装。".to_string());
        }
        let installer = state.updater.verified_installer().await?;
        std::process::Command::new(installer)
            .arg("--auto-update")
            .arg(format!("--update-dir={}", directory.to_string_lossy()))
            .arg(format!("--wait-pid={}", std::process::id()))
            .spawn()
            .map_err(|_| "无法启动自动更新安装器，请重试。".to_string())?;
        Ok::<(), String>(())
    }
    .await;
    if let Err(error) = result {
        state.updater.install_failed();
        let _ = app.emit("snowdown://update", state.updater.status());
        return Err(error);
    }
    update_notice::dismiss(&app);
    app.exit(0);
    Ok(())
}

#[tauri::command]
async fn inspect_video(
    app: AppHandle,
    input: String,
    state: State<'_, Arc<AppState>>,
) -> Result<VideoInfo, String> {
    let source = sites::recognize(&input)?;
    state.notifications.show(
        &app,
        format!("link:{}", source.url),
        "发现视频链接",
        &format!("已识别{}视频链接，正在解析。", source.platform.label()),
    );
    if state.clipboard.manual(clipboard::sequence()) {
        clipboard_watch::emit(&app, &state);
    }
    inspect_source(&app, &state, &source).await
}

async fn inspect_source(
    app: &AppHandle,
    state: &AppState,
    source: &sites::Source,
) -> Result<VideoInfo, String> {
    if source.platform == Platform::Web {
        state.extractor.inspect(app, source).await
    } else if source.platform == Platform::Bilibili {
        bilibili::inspect(&source.url, &state.auth.cookie()).await
    } else {
        Ok(state.site_sessions.resolve(app, source, 0).await?.info)
    }
}

#[tauri::command]
async fn read_clipboard_link() -> Result<Option<sites::Source>, String> {
    tokio::task::spawn_blocking(|| {
        for attempt in 0..5 {
            match clipboard::read_text() {
                Ok(text) => return Ok(text.and_then(|text| sites::recognize(&text).ok())),
                Err(error) if attempt == 4 => return Err(error),
                Err(_) => std::thread::sleep(std::time::Duration::from_millis(25)),
            }
        }
        Ok(None)
    })
    .await
    .map_err(|_| "无法读取剪贴板。".to_string())?
}

#[tauri::command]
fn clipboard_status(state: State<'_, Arc<AppState>>) -> clipboard_watch::Snapshot {
    state.clipboard.status()
}

#[tauri::command]
fn acknowledge_clipboard(app: AppHandle, state: State<'_, Arc<AppState>>, id: u64) {
    if state.clipboard.acknowledge(id) {
        clipboard_watch::emit(&app, &state);
    }
}

#[tauri::command]
fn tray_action(app: AppHandle, action: tray_menu::Action) -> Result<(), String> {
    tray_menu::action(&app, action)
}
#[tauri::command]
fn create_tasks(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    request: CreateTaskRequest,
) -> Result<(), String> {
    let settings = state.settings.lock().unwrap().clone();
    let result = batch::enqueue(
        &state,
        vec![request],
        BatchOptions {
            download_dir: settings.download_dir,
            interval_secs: 0,
            save_cover: settings.save_cover,
            save_danmaku: settings.save_danmaku,
            save_subtitles: settings.save_subtitles,
        },
        false,
    )?;
    state.emit(&app);
    if result.added == 0 {
        return Err("所选分P已有相同的下载任务。".into());
    }
    Ok(())
}

#[tauri::command]
fn create_batch_tasks(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    request: CreateBatchRequest,
) -> Result<BatchResult, String> {
    let result = batch::enqueue(&state, request.items, request.options, true)?;
    state.emit(&app);
    Ok(result)
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
        let task = tasks
            .iter()
            .find(|t| t.id == id)
            .ok_or("任务不存在")?
            .clone();
        // tasks -> active matches the scheduler. Late patches cannot recreate a deleted task.
        state.storage.remove_task(&id)?;
        let active = state.active.lock().unwrap();
        let running = active.contains_key(&id);
        if let Some(cancel) = active.get(&id) {
            cancel.store(true, Ordering::Relaxed);
        }
        tasks.retain(|t| t.id != id);
        drop(active);
        if !running {
            tauri::async_runtime::spawn(task_files::cleanup(task.download_dir, task.id));
        }
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
    if !(1..=4).contains(&settings.concurrency)
        || settings.max_speed_mbps > 1024
        || !(1..=16).contains(&settings.download_connections)
    {
        return Err("任务并发数应为1至4，每文件连接数为1至16，限速为0至1024 MB/s。".into());
    }
    std::fs::create_dir_all(&settings.download_dir).map_err(|e| format!("无法使用此目录：{e}"))?;
    state.storage.save_settings(&settings)?;
    *state.settings.lock().unwrap() = settings;
    let settings = state.settings.lock().unwrap().clone();
    if state
        .clipboard
        .configure(settings.clipboard_enabled, settings.clipboard_auto_parse)
    {
        clipboard_watch::emit(&app, &state);
    }
    state.emit(&app);
    Ok(())
}

#[tauri::command]
async fn begin_login(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    platform: Option<Platform>,
) -> Result<qr_login::LoginQr, String> {
    let platform = platform.unwrap_or_default();
    let result = if platform == Platform::Bilibili {
        state.auth.begin().await.map(Into::into)
    } else {
        state.qr_login.begin(&app, platform).await
    };
    state.emit(&app);
    result
}

#[tauri::command]
async fn poll_login(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    id: String,
    platform: Option<Platform>,
) -> Result<qr_login::LoginPoll, String> {
    let platform = platform.unwrap_or_default();
    let result = if platform == Platform::Bilibili {
        qr_login::LoginPoll::state(state.auth.poll(&id).await?.state)
    } else {
        state.qr_login.poll(&app, platform, &id).await?
    };
    if result.state == "confirmed" {
        state.emit(&app);
    }
    Ok(result)
}

#[tauri::command]
fn cancel_login(app: AppHandle, state: State<'_, Arc<AppState>>, platform: Option<Platform>) {
    let platform = platform.unwrap_or_default();
    if platform == Platform::Bilibili {
        state.auth.cancel();
    } else {
        state.qr_login.cancel(&app, platform);
    }
}

#[tauri::command]
async fn logout(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    platform: Option<Platform>,
) -> Result<(), String> {
    let platform = platform.unwrap_or_default();
    if platform == Platform::Bilibili {
        state.auth.logout()?;
    } else {
        state.qr_login.cancel(&app, platform);
        state.site_sessions.logout(&app, platform).await?;
    }
    state.emit(&app);
    Ok(())
}

#[tauri::command]
async fn check_login(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    platform: Option<Platform>,
) -> Result<qr_login::LoginCheck, String> {
    let platform = platform.unwrap_or_default();
    let result = if platform == Platform::Bilibili {
        state.auth.validate().await;
        let auth = state.auth.snapshot();
        Ok(match auth.state.as_str() {
            "authenticated" => qr_login::LoginCheck::new("authenticated", "登录状态有效。"),
            "loggedout" | "expired" => {
                qr_login::LoginCheck::new("loggedout", "尚未登录或登录已失效，请扫码登录。")
            }
            _ => qr_login::LoginCheck::new(
                "unknown",
                "暂时无法确认登录状态，原有账号已保留，请稍后重试。",
            ),
        })
    } else {
        state.qr_login.check(&app, platform).await
    };
    state.emit(&app);
    result
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
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let data_dir = handle.path().app_data_dir()?;
            #[cfg(debug_assertions)]
            let data_dir = if std::env::var_os("SNOWDOWN_QR_VERIFY").is_some() {
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.tools/qr-qa-app-data")
            } else if std::env::var_os("SNOWDOWN_EXTENSION_EXPORT_VERIFY").is_some() {
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../.tools/extension-export-qa")
                    .join(uuid::Uuid::new_v4().to_string())
            } else if std::env::var_os("SNOWDOWN_MODE_MORPH_VERIFY").is_some() {
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../.tools/mode-morph-qa")
                    .join(uuid::Uuid::new_v4().to_string())
            } else if std::env::var_os("SNOWDOWN_LOGIN_CHECK_VERIFY").is_some() {
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../.tools/login-check-qa")
                    .join(uuid::Uuid::new_v4().to_string())
            } else if std::env::var_os("SNOWDOWN_BROWSER_VERIFY").is_some() {
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.tools/bridge-qa-data/app")
            } else if std::env::var_os("SNOWDOWN_BATCH_VERIFY").is_some() {
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.tools/batch-qa-app-data")
            } else if std::env::var_os("SNOWDOWN_UPDATE_LAYOUT").is_some() {
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.tools/update-ui-qa-data")
            } else if std::env::var_os("SNOWDOWN_CLIPBOARD_VERIFY").is_some() {
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.tools/clipboard-qa-app-data")
            } else if std::env::var_os("SNOWDOWN_MINIMAL_VERIFY").is_some() {
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../.tools/minimal-qa-data")
                    .join(uuid::Uuid::new_v4().to_string())
            } else {
                data_dir
            };
            std::fs::create_dir_all(&data_dir)?;
            let storage =
                Storage::open(&data_dir.join("snowdown.db")).map_err(std::io::Error::other)?;
            let default_download = handle
                .path()
                .download_dir()
                .unwrap_or_else(|_| data_dir.clone())
                .join("SnowDown");
            #[cfg(debug_assertions)]
            let default_download = if std::env::var_os("SNOWDOWN_MINIMAL_VERIFY").is_some()
                || std::env::var_os("SNOWDOWN_BROWSER_VERIFY").is_some()
                || std::env::var_os("SNOWDOWN_LOGIN_CHECK_VERIFY").is_some()
                || std::env::var_os("SNOWDOWN_MODE_MORPH_VERIFY").is_some()
                || std::env::var_os("SNOWDOWN_EXTENSION_EXPORT_VERIFY").is_some()
            {
                data_dir.join("media")
            } else {
                default_download
            };
            let updater_version = env!("CARGO_PKG_VERSION");
            #[cfg(debug_assertions)]
            let updater_version = if std::env::var_os("SNOWDOWN_MINIMAL_VERIFY").is_some() {
                "0.0.0"
            } else {
                updater_version
            };
            let settings = storage
                .load_settings(Settings {
                    download_dir: default_download.to_string_lossy().to_string(),
                    concurrency: 2,
                    max_speed_mbps: 0,
                    download_connections: 8,
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
                qr_login: qr_login::QrLogin::default(),
                active: Mutex::new(HashMap::new()),
                storage,
                ffmpeg: find_ffmpeg(&handle),
                updater: Arc::new(updater::Updater::new(
                    data_dir.join("updates"),
                    updater_version,
                )),
                clipboard: clipboard_watch::Monitor::default(),
                stopping: AtomicBool::new(false),
                update_notice: update_notice::Notice::default(),
                browser: browser_bridge::Bridge::default(),
                notifications: notifications::Notifications::default(),
                extractor: web_extractor::Extractor::default(),
            });
            app.manage(state.clone());
            notifications::prepare(&handle, &data_dir);
            tray_menu::create(&handle)?;
            update_notice::create(&handle)?;
            #[cfg(debug_assertions)]
            let browser_test = std::env::var_os("SNOWDOWN_MINIMAL_VERIFY").is_some()
                || std::env::var_os("SNOWDOWN_LOGIN_CHECK_VERIFY").is_some()
                || std::env::var_os("SNOWDOWN_MODE_MORPH_VERIFY").is_some()
                || std::env::var_os("SNOWDOWN_EXTENSION_EXPORT_VERIFY").is_some();
            #[cfg(not(debug_assertions))]
            let browser_test = false;
            if !browser_test {
                let _ = browser_bridge::register();
                browser_bridge::start(handle.clone(), state.clone());
            }
            let cleanup_dir = data_dir.join("updates");
            tauri::async_runtime::spawn(async move {
                // The installer can still hold its own executable briefly after launch.
                for _ in 0..30 {
                    update_cleanup::installed(&cleanup_dir, updater_version).await;
                    if !browser_test {
                        browser_bridge::cleanup_hosts();
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            });
            #[cfg(debug_assertions)]
            if std::env::var_os("SNOWDOWN_EXTENSION_EXPORT_VERIFY").is_some() {
                #[cfg(windows)]
                extension_package_probe::start(handle.clone());
            } else if std::env::var_os("SNOWDOWN_MODE_MORPH_VERIFY").is_some() {
                #[cfg(windows)]
                mode_morph_probe::start(handle.clone());
            } else if std::env::var_os("SNOWDOWN_LOGIN_CHECK_VERIFY").is_some() {
                #[cfg(windows)]
                login_check_probe::start(handle.clone(), data_dir.clone());
            } else if std::env::var_os("SNOWDOWN_BROWSER_VERIFY").is_some() {
                let app = handle.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(6)).await;
                    let _ = minimal_probe::capture_label(&app, "main", "desktop-tile").await;
                    let _ = batch_probe::evaluate(
                        &app,
                        "document.querySelector('.extension-tile')?.click();true",
                    )
                    .await;
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    let _ = minimal_probe::capture_label(&app, "main", "extension-setup").await;
                    let _ = batch_probe::evaluate(
                        &app,
                        "document.querySelector('[aria-label=\"关闭插件设置\"]')?.click();true",
                    )
                    .await;
                });
            } else if std::env::var_os("SNOWDOWN_MINIMAL_VERIFY").is_some() {
                minimal_probe::start(handle.clone());
            } else if std::env::var_os("SNOWDOWN_CLIPBOARD_VERIFY").is_some() {
                clipboard_probe::start(handle.clone());
            } else if std::env::var_os("SNOWDOWN_BATCH_VERIFY").is_some() {
                batch_probe::start(handle.clone());
            } else {
                debug_probe::start(handle.clone());
            }
            let mut tray = TrayIconBuilder::with_id("main-tray")
                .tooltip("SnowDown · 好内容，慢慢收藏")
                .show_menu_on_left_click(false)
                .on_tray_icon_event(|tray, event| match event {
                    TrayIconEvent::Click {
                        button: MouseButton::Right,
                        button_state: MouseButtonState::Up,
                        position,
                        ..
                    } => {
                        let _ = tray_menu::show(tray.app_handle(), position);
                    }
                    TrayIconEvent::DoubleClick {
                        button: MouseButton::Left,
                        ..
                    } => show_main(tray.app_handle()),
                    _ => (),
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;
            clipboard_watch::start(handle.clone(), state.clone());
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
                if matches!(window.label(), "main" | "tray-menu" | "update-notice") {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            update_status,
            check_update,
            defer_update,
            download_update,
            cancel_update_download,
            install_update,
            dismiss_update_notice,
            browser_status,
            install_browser_extension,
            export_browser_extension_zip,
            open_update_release,
            inspect_video,
            create_tasks,
            create_batch_tasks,
            task_action,
            save_settings,
            begin_login,
            poll_login,
            cancel_login,
            logout,
            check_login,
            read_clipboard_link,
            clipboard_status,
            acknowledge_clipboard,
            tray_action,
            open_download_directory
        ])
        .run(tauri::generate_context!())
        .expect("SnowDown could not start");
}
