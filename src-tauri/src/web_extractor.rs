//! Site compatibility is delegated to the bundled, pinned extractor. Browser
//! captures contain only media metadata; configs, external plugins and remote
//! components are disabled. Child processes belong to an app-owned job.
use crate::{model::*, sites, AppState};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    process::Stdio,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::{Duration, Instant},
};
use tauri::{AppHandle, Manager};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, BufReader},
    process::{Child, Command},
};

async fn inspect_output(mut cmd: Command, budget_secs: u64) -> Result<Value, String> {
    let mut child = cmd.spawn().map_err(|_| "无法启动多站点解析组件。")?;
    let _tree = ProcessTree::attach(&child)?;
    let stdout = child.stdout.take().ok_or("解析输出不可用。")?;
    let stderr = child.stderr.take().ok_or("解析输出不可用。")?;
    let result = tokio::time::timeout(Duration::from_secs(budget_secs), async {
        let out = async {
            let mut bytes = vec![];
            stdout
                .take(8 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)
                .await
                .map(|_| bytes)
        };
        let err = async {
            let mut bytes = vec![];
            stderr
                .take(8192)
                .read_to_end(&mut bytes)
                .await
                .map(|_| bytes)
        };
        tokio::join!(child.wait(), out, err)
    })
    .await
    .map_err(|_| "网站解析超时，请在浏览器播放目标视频后使用插件读取。")?;
    let status = result.0.map_err(|_| "解析组件异常退出。")?;
    let bytes = result.1.map_err(|_| "解析输出读取失败。")?;
    let errors = result.2.unwrap_or_default();
    if !status.success() {
        return Err(safe_error(&errors));
    }
    if bytes.len() > 8 * 1024 * 1024 {
        return Err("网站返回数据过大。".into());
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| "网站视频数据无效。")?;
    Ok(value)
}

fn is_youtube(source: &sites::Source) -> bool {
    url::Url::parse(&source.url).ok().is_some_and(|u| {
        sites::host_matches(u.host_str().unwrap_or_default(), "youtube.com")
            || u.host_str() == Some("youtu.be")
    })
}

pub struct Extractor {
    cache: Mutex<HashMap<String, (Instant, VideoInfo, Option<Value>)>>,
    gate: tokio::sync::Semaphore,
}
impl Default for Extractor {
    fn default() -> Self {
        Self::new()
    }
}

fn resources(app: &AppHandle) -> Result<PathBuf, String> {
    let mut paths = vec![app
        .path()
        .resource_dir()
        .map_err(|_| "解析组件目录不可用。")?
        .join("resources")];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            paths.push(parent.join("resources"));
        }
    }
    #[cfg(debug_assertions)]
    paths.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources"));
    paths
        .into_iter()
        .find(|p| p.join("yt-dlp.exe").is_file() && p.join("deno.exe").is_file())
        .ok_or("多站点解析组件缺失，请安装完整客户端。".into())
}
fn command(app: &AppHandle) -> Result<Command, String> {
    let root = resources(app)?;
    let mut cmd = Command::new(root.join("yt-dlp.exe"));
    cmd.args([
        "--ignore-config",
        "--no-plugin-dirs",
        "--no-remote-components",
        "--no-cache-dir",
        "--no-playlist",
        "--no-colors",
        "--no-warnings",
        "--encoding",
        "utf-8",
        "--socket-timeout",
        "15",
        "--extractor-retries",
        "1",
        "--retries",
        "2",
        "--fragment-retries",
        "2",
        "--no-js-runtimes",
        "--js-runtimes",
    ])
    .arg(format!("deno:{}", root.join("deno.exe").display()))
    .arg("--ffmpeg-location")
    .arg(&root)
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(0x08000000);
    Ok(cmd)
}

#[cfg(windows)]
struct ProcessTree(windows_sys::Win32::Foundation::HANDLE);
#[cfg(windows)]
unsafe impl Send for ProcessTree {}
#[cfg(windows)]
impl ProcessTree {
    fn attach(child: &Child) -> Result<Self, String> {
        unsafe {
            use windows_sys::Win32::System::JobObjects::*;
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                return Err("无法创建解析进程组。".into());
            }
            let guard = Self(job);
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let handle = child.raw_handle().ok_or("解析进程已经退出。")?;
            if SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as _,
                std::mem::size_of_val(&limits) as u32,
            ) == 0
                || AssignProcessToJobObject(job, handle as _) == 0
            {
                return Err("无法隔离解析进程组。".into());
            }
            Ok(guard)
        }
    }
}
#[cfg(windows)]
impl Drop for ProcessTree {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}
#[cfg(not(windows))]
struct ProcessTree;
#[cfg(not(windows))]
impl ProcessTree {
    fn attach(_: &Child) -> Result<Self, String> {
        Ok(Self)
    }
}

fn safe_error(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes).to_lowercase();
    if text.contains("drm") || text.contains("encrypted") {
        return "此视频使用受保护的播放格式，无法下载。".into();
    }
    if text.contains("login")
        || text.contains("sign in")
        || text.contains("cookies")
        || text.contains("403")
        || text.contains("captcha")
    {
        return "此网站要求浏览器会话或验证，请在浏览器播放目标视频，再使用 SnowDown 插件读取并下载。".into();
    }
    "当前网站尚未提供可下载的视频数据。请在浏览器播放目标视频后用插件读取，或稍后重试。".into()
}
pub fn metadata(source: &sites::Source, data: &Value) -> Result<VideoInfo, String> {
    let site = url::Url::parse(&source.url)
        .ok()
        .and_then(|u| sites::catalog_site(u.host_str().unwrap_or_default()))
        .filter(|site| !site.id.is_empty())
        .ok_or("网站不在支持目录中。")?;
    if source.platform != Platform::Web
        || data["entries"].is_array()
        || data["is_live"] == true
        || data["has_drm"] == true
    {
        return Err("请选择单个可访问的点播视频。".into());
    }
    let mut qualities = BTreeMap::new();
    if let Some(formats) = data["formats"].as_array() {
        for format in formats.iter().take(1000) {
            if format["has_drm"] == true
                || format["vcodec"] == "none"
                || format["format_id"] == "storyboard"
            {
                continue;
            }
            let protocol = format["protocol"].as_str().unwrap_or("");
            if protocol.contains("drm") {
                continue;
            }
            let height = format["height"].as_u64().unwrap_or(0).min(16384) as u32;
            if !format["url"]
                .as_str()
                .is_some_and(|u| crate::resource_download::validate(u).is_ok())
            {
                continue;
            }
            qualities.insert(
                height,
                Quality {
                    id: height,
                    label: if height > 0 {
                        format!("{height}P")
                    } else {
                        "原始画质".into()
                    },
                    width: format["width"].as_u64().unwrap_or(0),
                    height: height as u64,
                },
            );
        }
    }
    if qualities.is_empty()
        && data["url"]
            .as_str()
            .is_some_and(|u| crate::resource_download::validate(u).is_ok())
    {
        qualities.insert(
            0,
            Quality {
                id: 0,
                label: if data["vcodec"] == "none" {
                    "音频".into()
                } else {
                    "原始画质".into()
                },
                width: 0,
                height: 0,
            },
        );
    }
    if qualities.is_empty() {
        return Err("未找到可下载画质，请在浏览器播放后通过插件读取。".into());
    }
    let title = data["title"]
        .as_str()
        .filter(|t| !t.is_empty())
        .unwrap_or("网页视频")
        .chars()
        .take(300)
        .collect::<String>();
    let duration = data["duration"]
        .as_f64()
        .filter(|v| v.is_finite() && *v > 0.)
        .unwrap_or(0.) as u64;
    let cover = data["thumbnail"]
        .as_str()
        .filter(|u| crate::resource_download::validate(u).is_ok())
        .unwrap_or_default()
        .to_string();
    Ok(VideoInfo {
        platform: Platform::Web,
        source_url: source.url.clone(),
        bvid: sites::video_id(source)?,
        title: title.clone(),
        cover,
        author: data["uploader"]
            .as_str()
            .filter(|s| !s.is_empty())
            .unwrap_or(&site.label)
            .chars()
            .take(100)
            .collect(),
        duration,
        pages: vec![VideoPage {
            cid: 1,
            page: 1,
            part: title,
            duration,
        }],
        qualities: qualities.into_values().rev().collect(),
        selected_cid: 1,
    })
}
impl Extractor {
    pub fn new() -> Self {
        Self {
            cache: Mutex::new(HashMap::new()),
            gate: tokio::sync::Semaphore::new(2),
        }
    }
    fn remembered(&self, url: &str) -> Option<(VideoInfo, Option<Value>)> {
        self.cache
            .lock()
            .unwrap()
            .get(url)
            .filter(|(at, _, _)| at.elapsed() < Duration::from_secs(180))
            .map(|(_, info, data)| (info.clone(), data.clone()))
    }
    fn remember(&self, source: &sites::Source, info: VideoInfo, data: Option<Value>) {
        let mut cache = self.cache.lock().unwrap();
        cache.retain(|_, (at, _, _)| at.elapsed() < Duration::from_secs(180));
        if cache.len() >= 64 {
            cache.clear();
        }
        cache.insert(source.url.clone(), (Instant::now(), info, data));
    }
    pub fn capture(&self, source: &sites::Source, value: &Value) -> Result<VideoInfo, String> {
        let media = &value["webMedia"];
        let claimed = media["sourceUrl"].as_str().and_then(|raw| {
            url::Url::parse(raw).ok()?;
            sites::recognize(raw).ok()
        });
        if source.platform != Platform::Web
            || !claimed
                .is_some_and(|value| value.platform == Platform::Web && value.url == source.url)
        {
            return Err("浏览器视频与目标链接不一致。".into());
        }
        let streams = media["streams"]
            .as_array()
            .filter(|s| !s.is_empty() && s.len() <= 64)
            .ok_or("网页视频数据无效。")?;
        let mut formats = vec![];
        for (i, s) in streams.iter().enumerate() {
            let url = s["url"].as_str().ok_or("媒体地址无效。")?;
            crate::resource_download::validate(url)?;
            let height = s["height"].as_u64().unwrap_or(0).min(16384);
            let hls =
                url.split('?').next().unwrap_or(url).ends_with(".m3u8") || s["kind"] == "manifest";
            let master = s["masterUrl"].as_str().unwrap_or(url);
            crate::resource_download::validate(master)?;
            formats.push(json!({"format_id":format!("capture-{i}"),"url":url,"manifest_url":if hls{Some(master)}else{None},"ext":if s["ext"]=="webm"{"webm"}else{"mp4"},"protocol":if hls{"m3u8_native"}else{"https"},"height":height,"width":s["width"].as_u64().unwrap_or(0).min(16384),"vcodec":"h264","acodec":if s["audio"].is_string(){"none"}else{"aac"}}));
            if let Some(audio) = s["audio"].as_str() {
                crate::resource_download::validate(audio)?;
                formats.push(json!({"format_id":format!("capture-audio-{i}"),"url":audio,"ext":"m4a","protocol":"https","vcodec":"none","acodec":"aac"}));
            }
        }
        let data = json!({"id":sites::video_id(source)?,"title":media["title"],"thumbnail":media["cover"],"uploader":media["author"],"duration":media["duration"],"webpage_url":source.url,"extractor":"generic","extractor_key":"Generic","formats":formats,"http_headers":{"Referer":source.url}});
        let info = metadata(source, &data)?;
        self.remember(source, info.clone(), Some(data));
        Ok(info)
    }
    pub async fn inspect(
        &self,
        app: &AppHandle,
        source: &sites::Source,
    ) -> Result<VideoInfo, String> {
        if let Some((info, _)) = self.remembered(&source.url) {
            return Ok(info);
        }
        let _guard = self.gate.acquire().await.map_err(|_| "解析组件已停止。")?;
        if let Some((info, _)) = self.remembered(&source.url) {
            return Ok(info);
        }
        crate::resource_download::validate(&source.url)?;
        let proxy = crate::extractor_proxy::start().await?;
        let mut cmd = command(app)?;
        cmd.arg("--proxy")
            .arg(&proxy.url)
            .env("NO_PROXY", "")
            .env("no_proxy", "");
        cmd.args(["--dump-single-json", "--skip-download", "--"])
            .arg(&source.url);
        let youtube = is_youtube(source);
        let value = if youtube {
            let mut quick = command(app)?;
            quick
                .arg("--proxy")
                .arg(&proxy.url)
                .env("NO_PROXY", "")
                .env("no_proxy", "");
            quick
                .args([
                    "--extractor-args",
                    "youtube:player_client=visionos;skip=hls,dash",
                    "--dump-single-json",
                    "--skip-download",
                    "--",
                ])
                .arg(&source.url);
            match inspect_output(quick, 12).await {
                Ok(value) if metadata(source, &value).is_ok() => value,
                _ => inspect_output(cmd, 45).await?,
            }
        } else {
            inspect_output(cmd, 60).await?
        };
        let info = metadata(source, &value)?;
        // Keep solved YouTube formats in the short-lived cache. Downloading
        // immediately after inspection must not repeat player challenges.
        self.remember(source, info.clone(), youtube.then_some(value));
        Ok(info)
    }
}

pub async fn run(
    app: &AppHandle,
    state: &AppState,
    task: &DownloadTask,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let source = sites::recognize(&task.source_url)?;
    if source.platform != Platform::Web || sites::video_id(&source)? != task.bvid {
        return Err("下载任务来源不一致。".into());
    }
    let root = PathBuf::from(&task.download_dir);
    let temp = root.join(".snowdown").join(&task.id);
    tokio::fs::create_dir_all(&temp)
        .await
        .map_err(|_| "无法创建下载目录。")?;
    let settings = state.settings.lock().unwrap().clone();
    let proxy = crate::extractor_proxy::start().await?;
    let mut cmd = command(app)?;
    cmd.arg("--proxy")
        .arg(&proxy.url)
        .args(["--downloader", "native"])
        .env("NO_PROXY", "")
        .env("no_proxy", "");
    cmd.args([
        "--newline",
        "--progress",
        "--continue",
        "--part",
        "--no-mtime",
        "--no-overwrites",
        "--progress-template",
        "download:SD_PROGRESS:{\"filename\":%(progress.filename)j,\"downloaded_bytes\":%(progress.downloaded_bytes|0)j,\"total_bytes\":%(progress.total_bytes,progress.total_bytes_estimate|0)j,\"speed\":%(progress.speed|0)j}",
        "--progress-template",
        "postprocess:SD_MERGE:%(progress.status)s",
        "--print",
        "after_move:SD_OUTPUT:%(filepath)j",
        "--concurrent-fragments",
    ])
    .arg(settings.download_connections.clamp(1, 16).to_string());
    if settings.max_speed_mbps > 0 {
        cmd.arg("--limit-rate")
            .arg((settings.max_speed_mbps as u64 * 1024 * 1024).to_string());
    }
    cmd.arg("-P").arg(&temp).args(["-o", "media.%(ext)s"]);
    cmd.env("TMP", &temp).env("TEMP", &temp);
    if task.format == OutputFormat::Audio {
        cmd.args([
            "-f",
            "bestaudio/best",
            "--extract-audio",
            "--audio-format",
            "m4a",
        ]);
    } else {
        let selector = if task.quality > 0 {
            format!(
                "bestvideo[height<={}]+bestaudio/best[height<={}]/best",
                task.quality, task.quality
            )
        } else {
            "bestvideo+bestaudio/best".into()
        };
        cmd.args([
            "-f",
            &selector,
            "--merge-output-format",
            "mp4",
            "--remux-video",
            "mp4",
        ]);
    }
    let captured = temp.join("browser-media.json");
    if let Some((_, Some(data))) = state.extractor.remembered(&source.url) {
        tokio::fs::write(
            &captured,
            serde_json::to_vec(&data).map_err(|_| "网页视频数据无效。")?,
        )
        .await
        .map_err(|_| "无法保存下载进度。")?;
    }
    let manifest = if captured.is_file() {
        tokio::fs::read(&captured)
            .await
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .and_then(|data| {
                // Browser captures need master-playlist expansion; native
                // extractor JSON already contains complete audio/video formats.
                if data["extractor"] != "generic" {
                    return None;
                }
                data["formats"].as_array().and_then(|f| {
                    f.iter()
                        .find_map(|f| f["manifest_url"].as_str().map(str::to_owned))
                })
            })
    } else {
        None
    };
    if let Some(manifest) = manifest {
        crate::resource_download::validate(&manifest)?;
        cmd.arg("--referer")
            .arg(&source.url)
            .arg("--")
            .arg(manifest);
    } else if captured.is_file() {
        cmd.arg("--load-info-json").arg(&captured);
    } else {
        cmd.arg("--").arg(&source.url);
    }
    let mut child = cmd.spawn().map_err(|_| "无法启动下载组件。")?;
    let _tree = ProcessTree::attach(&child)?;
    let mut lines = BufReader::new(child.stdout.take().ok_or("下载输出不可用。")?).lines();
    let stderr = child.stderr.take().ok_or("下载输出不可用。")?;
    let errors = tokio::spawn(async move {
        let mut bytes = vec![];
        let _ = stderr.take(8192).read_to_end(&mut bytes).await;
        bytes
    });
    let mut output = None;
    let mut last_progress = Instant::now() - Duration::from_secs(1);
    let mut downloaded = HashMap::new();
    let mut totals = HashMap::new();
    loop {
        tokio::select! {
            line=lines.next_line()=>{let Some(line)=line.map_err(|_|"下载进度读取失败。")?else{break};
                if let Some(raw)=line.strip_prefix("SD_OUTPUT:"){output=serde_json::from_str::<String>(raw).ok().map(PathBuf::from);}
                else if line.starts_with("SD_MERGE:"){let _=state.patch_task(app,&task.id,|t|{t.status=TaskStatus::Merging;t.speed=0;});}
                else if let Some(raw)=line.strip_prefix("SD_PROGRESS:"){if let Ok(stats)=serde_json::from_str::<Value>(raw){
                    let file=stats["filename"].as_str().unwrap_or("media").to_string();downloaded.insert(file.clone(),stats["downloaded_bytes"].as_u64().unwrap_or(0));totals.insert(file,stats["total_bytes"].as_u64().or_else(||stats["total_bytes_estimate"].as_u64()).unwrap_or(0));
                    if last_progress.elapsed()>Duration::from_millis(180){last_progress=Instant::now();let bytes=downloaded.values().sum::<u64>();let total=totals.values().sum::<u64>();let _=state.patch_task(app,&task.id,|t|{t.status=TaskStatus::Downloading;t.downloaded_bytes=bytes;t.total_bytes=total;t.progress=if total>0{(bytes as f64/total as f64*94.).min(94.)}else{0.};t.speed=stats["speed"].as_f64().unwrap_or(0.).max(0.) as u64;t.error=None;});}
                }}
            }
            _=tokio::time::sleep(Duration::from_millis(120))=>{if cancel.load(Ordering::Relaxed){let _=child.kill().await;errors.abort();return Err("下载已暂停，进度已保存。".into());}}
        }
    }
    let status = child.wait().await.map_err(|_| "下载组件异常退出。")?;
    let error = errors.await.unwrap_or_default();
    if cancel.load(Ordering::Relaxed) {
        return Err("下载已暂停，进度已保存。".into());
    }
    if !status.success() {
        return Err(safe_error(&error));
    }
    let output = output.ok_or("下载组件没有返回完成文件。")?;
    let actual = tokio::fs::canonicalize(&output)
        .await
        .map_err(|_| "下载结果不存在。")?;
    let owned = tokio::fs::canonicalize(&temp)
        .await
        .map_err(|_| "下载缓存不存在。")?;
    if actual.parent() != Some(owned.as_path())
        || !actual
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with("media."))
    {
        return Err("下载结果超出任务目录。".into());
    }
    let extension = actual
        .extension()
        .and_then(|e| e.to_str())
        .filter(|e| ["mp4", "m4a", "webm", "mkv", "mp3"].contains(e))
        .ok_or("下载结果格式无效。")?;
    let target = root.join(format!(
        "{}-{}.{}",
        safe_filename(&task.title),
        &task.id[..8],
        extension
    ));
    let bytes = tokio::fs::metadata(&actual)
        .await
        .map_err(|_| "下载结果不可读。")?
        .len();
    if bytes == 0 {
        return Err("下载结果为空。".into());
    }
    tokio::fs::rename(&actual, &target)
        .await
        .map_err(|_| "无法保存下载结果。")?;
    state.patch_task(app, &task.id, |t| {
        t.status = TaskStatus::Completed;
        t.progress = 100.;
        t.downloaded_bytes = bytes;
        t.total_bytes = bytes;
        t.speed = 0;
        t.error = None;
        t.output_path = Some(target.to_string_lossy().into_owned());
    })?;
    if task.save_cover && !task.cover.is_empty() {
        let result = tokio::time::timeout(
            Duration::from_secs(12),
            save_cover(&task.cover, &source.url, &target, &proxy.url),
        )
        .await;
        if !matches!(result, Ok(Ok(()))) {
            let _ = state.patch_task(app, &task.id, |t| {
                t.warning = Some("视频已保存，封面暂时无法获取。".into())
            });
        }
    }
    crate::task_files::cleanup(task.download_dir.clone(), task.id.clone()).await;
    Ok(())
}

async fn save_cover(
    url: &str,
    source: &str,
    output: &std::path::Path,
    proxy: &str,
) -> Result<(), String> {
    crate::resource_download::validate(url)?;
    let client = reqwest::Client::builder()
        .proxy(reqwest::Proxy::all(proxy).map_err(|_| "下载连接无效。")?)
        .user_agent(crate::bilibili::USER_AGENT)
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() < 5 && attempt.url().scheme() == "https" {
                attempt.follow()
            } else {
                attempt.error("封面跳转无效")
            }
        }))
        .build()
        .map_err(|_| "封面连接失败。")?;
    let mut response = client
        .get(url)
        .header(reqwest::header::REFERER, source)
        .send()
        .await
        .map_err(|_| "封面暂时不可用。")?
        .error_for_status()
        .map_err(|_| "封面暂时不可用。")?;
    let mut bytes = vec![];
    while let Some(chunk) = response.chunk().await.map_err(|_| "封面读取失败。")? {
        if bytes.len() + chunk.len() > 8 * 1024 * 1024 {
            return Err("封面过大。".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let extension = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "png"
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        "jpg"
    } else if bytes.starts_with(b"GIF8") {
        "gif"
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        "webp"
    } else {
        return Err("封面格式无效。".into());
    };
    tokio::fs::write(output.with_extension(extension), bytes)
        .await
        .map_err(|_| "封面保存失败。".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_filters_drm_and_keeps_real_qualities() {
        let source = sites::recognize("https://www.youtube.com/watch?v=fixture").unwrap();
        let data = json!({"title":"fixture","formats":[{"url":"https://cdn.example.com/a.mp4","height":1080,"vcodec":"h264"},{"url":"https://cdn.example.com/b.mp4","height":720,"vcodec":"h264"},{"url":"https://cdn.example.com/c.mp4","height":2160,"has_drm":true},{"url":"https://127.0.0.1/x","height":4320}]});
        let info = metadata(&source, &data).unwrap();
        assert_eq!(
            info.qualities.iter().map(|q| q.id).collect::<Vec<_>>(),
            vec![1080, 720]
        );
        assert_eq!(info.bvid, sites::video_id(&source).unwrap());
    }
    #[test]
    fn capture_requires_matching_source_and_public_media() {
        let parser = Extractor::new();
        let source = sites::recognize("https://www.instagram.com/reel/fixture/").unwrap();
        let valid = json!({"webMedia":{"sourceUrl":source.url,"title":"fixture","streams":[{"url":"https://cdn.example.com/a.mp4","height":720}]}});
        assert_eq!(
            parser.capture(&source, &valid).unwrap().qualities[0].id,
            720
        );
        let mut other = valid.clone();
        other["webMedia"]["sourceUrl"] = json!("https://www.instagram.com/reel/other/");
        assert!(parser.capture(&source, &other).is_err());
        let mut fragment = valid.clone();
        fragment["webMedia"]["sourceUrl"] = json!(format!("{}#t=10", source.url));
        assert!(parser.capture(&source, &fragment).is_ok());
        let mut private = valid;
        private["webMedia"]["streams"][0]["url"] = json!("https://127.0.0.1/a.mp4");
        assert!(parser.capture(&source, &private).is_err());
    }
}
