//! Native messaging is restricted to our store and local extension IDs. No HTTP server or
//! external-webpage messaging is exposed. The local broker requires a DPAPI token.
use crate::{auth, batch, model::*, site_sessions, AppState};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::{
    collections::HashMap,
    io::{Read, Write},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tauri::{AppHandle, Manager};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
pub const ID: &str = "dbhbjidcgjfmcoepcechajmoofdelkno";
const STORE_ID: &str = "npedaeojbodmkfiaicobfdakidgafdim";
const STORE_URL: &str = "https://chromewebstore.google.com/detail/snowdown/npedaeojbodmkfiaicobfdakidgafdim";
fn allowed_origins() -> Vec<String> {
    [ID, STORE_ID].iter().map(|id| format!("chrome-extension://{id}/")).collect()
}
fn trusted_origin(origin: &str) -> bool {
    allowed_origins().iter().any(|allowed| allowed == origin)
}
const HOST: &str = "app.snowdown.bridge";
const MAX: usize = 512 * 1024;
static BINDING_WRITE: Mutex<()> = Mutex::new(());
#[derive(Default)]
pub struct Bridge {
    clients: Mutex<HashMap<String, Client>>,
}
struct Client { seen: Instant, version: Option<String>, revision: Option<String> }
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserStatus {
    id: String,
    name: String,
    installed: bool,
    connected: bool,
    extension_version: Option<String>,
    bundled_version: String,
    update_available: bool,
    refresh_required: bool,
}
fn extension_status(version: Option<&str>, revision: Option<&str>, bundled: &str) -> (bool,bool) {
    let outdated=version.and_then(|v|semver::Version::parse(v).ok()).zip(semver::Version::parse(bundled).ok()).is_some_and(|(installed,current)|installed<current);
    (outdated,version.is_none() || version==Some(bundled)&&revision!=Some("desktop-0.2.15"))
}
#[cfg(test)]
mod version_tests {
    use super::*;
    #[test]
    fn only_exact_store_and_local_origins_can_start_native_messaging() {
        assert_eq!(allowed_origins().len(), 2);
        for id in [ID, STORE_ID] {
            assert!(trusted_origin(&format!("chrome-extension://{id}/")));
            assert!(!trusted_origin(&format!("chrome-extension://{id}/other")));
            assert!(!trusted_origin(&format!("https://{id}/")));
            assert!(!trusted_origin(&format!("chrome-extension://{id}.evil/")));
        }
        assert!(!trusted_origin("chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/"));
        assert!(!trusted_origin(""));
    }
    #[test]
    fn installed_extension_is_distinct_from_bundled_files() {
        assert_eq!(extension_status(Some("0.1.5"),None,"0.1.6"),(true,false));
        assert_eq!(extension_status(None,None,"0.1.6"),(false,true));
        assert_eq!(extension_status(Some("0.1.6"),None,"0.1.6"),(false,true));
        assert_eq!(extension_status(Some("0.1.6"),Some("desktop-0.2.15"),"0.1.6"),(false,false));
        assert_eq!(extension_status(Some("0.1.7"),None,"0.1.6"),(false,false));
    }
}
#[derive(Serialize, Deserialize)]
struct Endpoint {
    port: u16,
    token: String,
}
fn root() -> PathBuf {
    #[cfg(debug_assertions)]
    if std::env::var_os("SNOWDOWN_EXTENSION_EXPORT_VERIFY").is_some() {
        return PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../.tools/extension-export-bridge-qa");
    }
    #[cfg(debug_assertions)]
    if std::env::var_os("SNOWDOWN_BROWSER_VERIFY").is_some() {
        return PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join(".tools/bridge-qa-data");
    }
    PathBuf::from(std::env::var_os("APPDATA").unwrap_or_default())
        .join("app.snowdown.desktop")
        .join("browser-link")
}
fn browser(id: &str) -> Option<PathBuf> {
    let relative = match id {
        "chrome" => "Google/Chrome/Application/chrome.exe",
        "edge" => "Microsoft/Edge/Application/msedge.exe",
        _ => return None,
    };
    ["PROGRAMFILES", "PROGRAMFILES(X86)", "LOCALAPPDATA"]
        .iter()
        .filter_map(|key| std::env::var_os(key))
        .map(|base| PathBuf::from(base).join(relative))
        .find(|p| p.is_file())
}
fn reg(program: &str, manifest: &std::path::Path) -> Result<(), String> {
    let key = format!("HKCU\\Software\\{program}\\NativeMessagingHosts\\{HOST}");
    let mut command = std::process::Command::new("reg.exe");
    command
        .args(["ADD", &key, "/ve", "/t", "REG_SZ", "/d"])
        .arg(manifest)
        .arg("/f");
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    if command
        .output()
        .map_err(|_| "无法注册浏览器联动。")?
        .status
        .success()
    {
        Ok(())
    } else {
        Err("无法注册浏览器联动。".into())
    }
}
pub fn register() -> Result<(), String> {
    let directory = root();
    std::fs::create_dir_all(&directory).map_err(|_| "无法创建联动目录。")?;
    let manifest = directory.join("native-host.json");
    let executable = std::env::current_exe().map_err(|_| "无法确定程序位置。")?;
    use sha2::Digest;
    let mut source = std::fs::File::open(&executable).map_err(|_| "无法读取浏览器消息组件。")?;
    let mut hash = sha2::Sha256::new();
    let mut buffer = vec![0u8; 128 * 1024];
    loop {
        let n = source.read(&mut buffer).map_err(|_| "无法读取消息组件。")?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    let digest = format!("{:x}", hash.finalize());
    let host = directory.join(format!(
        "snowdown-host-{}-{}.exe",
        env!("CARGO_PKG_VERSION"),
        &digest[..16]
    ));
    if !host.is_file() {
        std::fs::copy(&executable, &host).map_err(|_| "无法准备浏览器消息组件。")?;
    }
    std::fs::write(&manifest,serde_json::to_vec(&json!({"name":HOST,"description":"SnowDown local browser connection","path":host,"type":"stdio","allowed_origins":allowed_origins()})).unwrap()).map_err(|_|"无法保存浏览器联动。")?;
    for program in ["Google\\Chrome", "Microsoft\\Edge"] {
        reg(program, &manifest)?;
    }
    cleanup_hosts();
    Ok(())
}
pub fn cleanup_hosts() {
    let directory = root();
    let manifest = std::fs::read(directory.join("native-host.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok());
    let keep = manifest
        .as_ref()
        .and_then(|m| m["path"].as_str())
        .map(PathBuf::from);
    let Ok(files) = std::fs::read_dir(&directory) else {
        return;
    };
    for entry in files.flatten() {
        if keep.as_ref() == Some(&entry.path()) || !entry.file_type().is_ok_and(|t| t.is_file()) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(value) = name
            .strip_prefix("snowdown-host-")
            .and_then(|s| s.strip_suffix(".exe"))
        else {
            continue;
        };
        let version = value
            .rsplit_once('-')
            .filter(|(_, hash)| hash.len() == 16 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
            .map(|(v, _)| v)
            .unwrap_or(value);
        if semver::Version::parse(version)
            .ok()
            .zip(semver::Version::parse(env!("CARGO_PKG_VERSION")).ok())
            .is_some_and(|(old, current)| old <= current)
        {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}
pub fn snapshot(state: &AppState) -> Vec<BrowserStatus> {
    let clients = state.browser.clients.lock().unwrap();
    let bundled=serde_json::from_str::<Value>(include_str!("../resources/browser-extension/manifest.json")).ok().and_then(|v|v["version"].as_str().map(str::to_owned)).unwrap_or_default();
    [("chrome", "Google Chrome"), ("edge", "Microsoft Edge")]
        .into_iter()
        .map(|(id, name)| {let client=clients.get(id).filter(|c|c.seen.elapsed()<Duration::from_secs(16));let version=client.and_then(|c|c.version.clone());let (update,refresh)=extension_status(version.as_deref(),client.and_then(|c|c.revision.as_deref()),&bundled);BrowserStatus {
            id: id.into(),
            name: name.into(),
            installed: browser(id).is_some(),
            connected: client.is_some(),extension_version:version,bundled_version:bundled.clone(),update_available:client.is_some()&&update,refresh_required:client.is_some()&&refresh,
        }})
        .collect()
}
pub fn install(app: &AppHandle, id: &str) -> Result<String, String> {
    use tauri_plugin_opener::OpenerExt;
    if !matches!(id, "chrome" | "edge") {
        return Err("浏览器类型无效。".into());
    }
    let Some(executable) = browser(id) else {
        app.opener()
            .open_url(
                if id == "chrome" {
                    "https://www.google.com/chrome/"
                } else {
                    "https://www.microsoft.com/edge/download"
                },
                None::<&str>,
            )
            .map_err(|_| "无法打开浏览器下载页面。")?;
        return Ok(String::new());
    };
    register()?;
    if id == "chrome" {
        ensure_bindings(&[id])?;
        std::process::Command::new(executable)
            .arg(STORE_URL)
            .spawn()
            .map_err(|_| "无法打开 Chrome 插件商店。")?;
        return Ok(STORE_URL.into());
    }
    let directory = root().join("extension");
    std::fs::create_dir_all(&directory).map_err(|_| "无法准备插件文件。")?;
    let source = app
        .path()
        .resource_dir()
        .map_err(|_| "插件资源不可用。")?
        .join("resources/browser-extension");
    fn copy(source: &std::path::Path, dest: &std::path::Path) -> Result<(), String> {
        for entry in std::fs::read_dir(source).map_err(|_| "插件文件缺失，请安装完整软件。")?
        {
            let entry = entry.map_err(|_| "无法读取插件文件。")?;
            let target = dest.join(entry.file_name());
            if entry.file_type().map_err(|_| "插件文件无效。")?.is_dir() {
                std::fs::create_dir_all(&target).map_err(|_| "无法准备插件目录。")?;
                copy(&entry.path(), &target)?;
            } else {
                std::fs::copy(entry.path(), target)
                    .map_err(|_| "无法更新插件文件，请稍后重试。")?;
            }
        }
        Ok(())
    }
    copy(&source, &directory)?;
    ensure_bindings(&[id])?;
    std::process::Command::new(executable)
        .arg(if id == "chrome" {
            "chrome://extensions/"
        } else {
            "edge://extensions/"
        })
        .spawn()
        .map_err(|_| "无法打开扩展管理页。")?;
    app.opener()
        .open_path(directory.to_string_lossy().to_string(), None::<&str>)
        .map_err(|_| "无法打开插件目录。")?;
    Ok(directory.to_string_lossy().into_owned())
}
fn ensure_bindings(ids: &[&str]) -> Result<(), String> {
    let _guard = BINDING_WRITE.lock().unwrap();
    std::fs::create_dir_all(root()).map_err(|_| "无法创建联动目录。")?;
    let path = root().join("bindings.json");
    let mut bindings: HashMap<String, String> = std::fs::read(&path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    let mut changed = false;
    for id in ids {
        bindings
            .entry((*id).into())
            .or_insert_with(|| { changed = true; uuid::Uuid::new_v4().to_string() });
    }
    if !changed { return Ok(()); }
    std::fs::write(path, serde_json::to_vec(&bindings).unwrap())
        .map_err(|_| "无法保存绑定配置。")?;
    Ok(())
}
pub fn prepare_zip_bindings() -> Result<(), String> {
    let ids: Vec<_> = ["chrome", "edge"]
        .into_iter()
        .filter(|id| browser(id).is_some())
        .collect();
    ensure_bindings(&ids)
}
pub fn native_host() -> bool {
    let args = std::env::args().collect::<Vec<_>>();
    if !args.iter().any(|a| a.starts_with("chrome-extension://")) {
        return false;
    }
    if !args.get(1).is_some_and(|origin| trusted_origin(origin)) {
        return true;
    }
    let mut input = std::io::stdin().lock();
    let mut output = std::io::stdout().lock();
    loop {
        let mut header = [0u8; 4];
        if input.read_exact(&mut header).is_err() {
            break;
        }
        let size = u32::from_le_bytes(header) as usize;
        if size == 0 || size > MAX {
            break;
        }
        let mut body = vec![0; size];
        if input.read_exact(&mut body).is_err() {
            break;
        }
        let request = serde_json::from_slice::<Value>(&body).unwrap_or(Value::Null);
        let id = request["id"].clone();
        let result = (|| -> Option<Value> {
            let encrypted = std::fs::read(root().join("endpoint.dat")).ok()?;
            if encrypted.len() > 4096 {
                return None;
            }
            let endpoint: Endpoint =
                serde_json::from_slice(&auth::crypt(&encrypted, false).ok()?).ok()?;
            let mut stream = std::net::TcpStream::connect_timeout(
                &std::net::SocketAddr::from(([127, 0, 0, 1], endpoint.port)),
                Duration::from_secs(1),
            )
            .ok()?;
            stream
                .set_read_timeout(Some(Duration::from_secs(95)))
                .ok()?;
            stream
                .set_write_timeout(Some(Duration::from_secs(5)))
                .ok()?;
            let bytes =
                serde_json::to_vec(&json!({"token":endpoint.token,"request":request})).ok()?;
            stream.write_all(&(bytes.len() as u32).to_le_bytes()).ok()?;
            stream.write_all(&bytes).ok()?;
            let mut header = [0; 4];
            stream.read_exact(&mut header).ok()?;
            let size = u32::from_le_bytes(header) as usize;
            if size > MAX {
                return None;
            }
            let mut result = vec![0; size];
            stream.read_exact(&mut result).ok()?;
            serde_json::from_slice(&result).ok()
        })()
        .unwrap_or_else(
            || json!({"id":id,"connected":false,"error":"SnowDown 未运行，已切换到浏览器下载。"}),
        );
        let bytes = serde_json::to_vec(&result).unwrap();
        if output
            .write_all(&(bytes.len() as u32).to_le_bytes())
            .and_then(|_| output.write_all(&bytes))
            .and_then(|_| output.flush())
            .is_err()
        {
            break;
        }
    }
    true
}
pub fn start(app: AppHandle, state: Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        let Ok(listener) = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await else {
            return;
        };
        let token = format!("{}{}", uuid::Uuid::new_v4(), uuid::Uuid::new_v4());
        let endpoint = Endpoint {
            port: listener.local_addr().unwrap().port(),
            token: token.clone(),
        };
        let Ok(encrypted) = auth::crypt(&serde_json::to_vec(&endpoint).unwrap(), true) else {
            return;
        };
        if tokio::fs::create_dir_all(root()).await.is_err()
            || tokio::fs::write(root().join("endpoint.dat"), encrypted)
                .await
                .is_err()
        {
            return;
        }
        let limit = Arc::new(tokio::sync::Semaphore::new(8));
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let Ok(permit) = limit.clone().try_acquire_owned() else {
                continue;
            };
            let app = app.clone();
            let state = state.clone();
            let token = token.clone();
            tauri::async_runtime::spawn(async move {
                let _permit = permit;
                let _ = tokio::time::timeout(Duration::from_secs(90), async {
                    let size = socket.read_u32_le().await.ok()? as usize;
                    if size == 0 || size > MAX {
                        return None;
                    }
                    let mut bytes = vec![0; size];
                    socket.read_exact(&mut bytes).await.ok()?;
                    let value: Value = serde_json::from_slice(&bytes).ok()?;
                    if value["token"].as_str() != Some(&token) {
                        return None;
                    }
                    let request = &value["request"];
                    let result = dispatch(&app, &state, request).await;
                    let response = match result {
                        Ok(data) => json!({"id":request["id"],"connected":true,"data":data}),
                        Err(error) => json!({"id":request["id"],"connected":true,"error":error}),
                    };
                    let bytes = serde_json::to_vec(&response).ok()?;
                    if bytes.len() > MAX {
                        return None;
                    }
                    socket.write_u32_le(bytes.len() as u32).await.ok()?;
                    socket.write_all(&bytes).await.ok()?;
                    Some(())
                })
                .await;
            });
        }
    });
}
async fn dispatch(app: &AppHandle, state: &AppState, request: &Value) -> Result<Value, String> {
    let browser = request["browser"]
        .as_str()
        .filter(|id| matches!(*id, "chrome" | "edge"))
        .ok_or("未知浏览器。")?;
    if request["method"] == "hello" {
        // Store installations can connect without first using the desktop install button.
        ensure_bindings(&[browser])?;
        state
            .browser
            .clients
            .lock()
            .unwrap()
            .insert(browser.into(), Client {seen:Instant::now(),version:request["extensionVersion"].as_str().filter(|v|v.len()<32&&semver::Version::parse(v).is_ok()).map(str::to_owned),revision:request["extensionRevision"].as_str().filter(|v|v.len()<64).map(str::to_owned)});
        let binding: HashMap<String, String> = std::fs::read(root().join("bindings.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        return Ok(
            json!({"version":env!("CARGO_PKG_VERSION"),"binding":binding.get(browser),"platforms":state.site_sessions.snapshot()}),
        );
    }
    if state.updater.installing() || state.stopping.load(std::sync::atomic::Ordering::Relaxed) {
        return Err("正在安装更新，请稍后重试。".into());
    }
    match request["method"].as_str() {
        Some("inspect") => {
            let source = crate::sites::recognize(request["url"].as_str().ok_or("视频链接无效。")?)?;
            let video = crate::inspect_source(app, state, &source).await?;
            Ok(
                json!({"title":video.title,"cover":video.cover,"author":video.author,"duration":video.duration,"streams":video.qualities.iter().map(|q|json!({"quality":q.id,"label":q.label,"width":q.width,"height":q.height,"native":true})).collect::<Vec<_>>() }),
            )
        }
        Some("resource-archive") => {
            let items:Vec<crate::model::ArchiveResource>=serde_json::from_value(request["items"].clone()).map_err(|_|"打包资源无效。")?;
            crate::resource_archive::enqueue(app,state,items,request["name"].as_str().unwrap_or("网页资源"))?;
            Ok(json!({"added":1}))
        }
        Some("resource") => {
            crate::resource_download::enqueue(
                app,
                state,
                request["url"].as_str().ok_or("资源地址无效。")?,
                request["name"].as_str().unwrap_or("浏览器资源"),
                request["kind"].as_str().unwrap_or(""),
            )?;
            Ok(json!({"added":1}))
        }
        Some("sessions") => {
            let sessions = request["sessions"]
                .as_array()
                .filter(|s| s.len() <= 3)
                .ok_or("会话数据无效。")?;
            let mut results = vec![];
            for session in sessions {
                let platform: Platform = serde_json::from_value(session["platform"].clone())
                    .map_err(|_| "平台无效。")?;
                if platform == Platform::Channels { return Err("视频号解析会话需在桌面端的官方页面登录，不接受浏览器账号同步。".into()); }
                let cookie = session["cookie"].as_str().unwrap_or_default().to_string();
                if cookie.is_empty() {
                    continue;
                }
                let result = if platform == Platform::Bilibili {
                    state.auth.import_browser(cookie).await
                } else {
                    let required = if platform == Platform::Douyin {
                        "sessionid"
                    } else {
                        "web_session"
                    };
                    if !cookie.split(';').any(|part| {
                        part.trim()
                            .split_once('=')
                            .is_some_and(|(key, value)| key == required && !value.is_empty())
                    }) {
                        Err("浏览器尚未登录。".into())
                    } else {
                        state.qr_login.cancel(app, platform);
                        state.site_sessions.import_browser(app,platform, cookie).await.map(|_| {
                            if let Ok(profile) = serde_json::from_value::<site_sessions::Profile>(
                                session["account"].clone(),
                            ) {
                                let _ = state.site_sessions.save_profile(platform, profile);
                            }
                        })
                    }
                };
                results.push(json!({"platform":platform,"saved":result.is_ok()}));
            }
            state.emit(app);
            Ok(json!(results))
        }
        Some("queue") => {
            let input = request["url"].as_str().ok_or("视频链接无效。")?;
            let source = crate::sites::recognize(input)?;
            let requested = request["quality"]
                .as_u64()
                .unwrap_or(0)
                .min(u32::MAX as u64) as u32;
            // Browser details are accepted only after the regular site/CDN validation.
            let web_video =
                if source.platform == Platform::Web && request["detail"]["webMedia"].is_object() {
                    Some(state.extractor.capture(&source, &request["detail"])?)
                } else {
                    None
                };
            let browser_video =
                if matches!(source.platform, Platform::Douyin | Platform::Xiaohongshu) {
                    request
                        .get("detail")
                        .filter(|d| d.is_object())
                        .map(|detail| {
                            state
                                .site_sessions
                                .remember_browser(&source, detail, requested)
                        })
                        .transpose()?
                } else {
                    None
                };
            let video = if let Some(video) = web_video {
                video
            } else if let Some(resolved) = &browser_video {
                resolved.info.clone()
            } else {
                crate::inspect_source(app, state, &source).await?
            };
            let quality = video
                .qualities
                .iter()
                .find(|q| {
                    q.id == browser_video
                        .as_ref()
                        .map(|v| v.quality)
                        .unwrap_or(requested)
                })
                .or(video.qualities.first())
                .ok_or("暂无可用画质。")?
                .id;
            let settings = state.settings.lock().unwrap().clone();
            let result = batch::enqueue(
                state,
                vec![CreateTaskRequest {
                    page_cids: vec![video.selected_cid],
                    quality,
                    format: if request["format"] == "audio" {
                        OutputFormat::Audio
                    } else {
                        OutputFormat::Video
                    },
                    video,
                }],
                BatchOptions {
                    download_dir: settings.download_dir,
                    interval_secs: 0,
                    save_cover: settings.save_cover,
                    save_danmaku: settings.save_danmaku,
                    save_subtitles: settings.save_subtitles,
                },
                false,
            )?;
            state.emit(app);
            Ok(json!(result))
        }
        _ => Err("不支持的插件操作。".into()),
    }
}
