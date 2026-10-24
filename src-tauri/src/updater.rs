use crate::model::now_ms;
use reqwest::{Client, StatusCode};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant},
};
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{watch, Mutex as AsyncMutex},
};
use url::Url;

const LATEST: &str = "https://api.github.com/repos/xiaogouyo/SnowDown/releases/latest";
const RELEASES: &str = "https://github.com/xiaogouyo/SnowDown/releases";
const MAX_INSTALLER: u64 = 512 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseInfo {
    pub version: String,
    pub title: String,
    pub notes: String,
    pub published_at: String,
    pub url: String,
    pub file_name: String,
    pub size: u64,
    #[serde(skip)]
    download_url: String,
    #[serde(skip)]
    sha256: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Idle,
    Downloading,
    Ready,
    Installing,
    Error,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub current_version: String,
    pub release: Option<ReleaseInfo>,
    pub phase: Phase,
    pub downloaded: u64,
    pub total: u64,
    pub error: Option<String>,
    pub deferred: bool,
}

#[derive(Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Preferences {
    version: String,
    remind_after: u64,
}

pub struct Updater {
    directory: PathBuf,
    status: Mutex<UpdateStatus>,
    preferences: Mutex<Preferences>,
    pub operation: AsyncMutex<()>,
    cancel: watch::Sender<u64>,
}

fn version(value: &str) -> Result<Version, String> {
    if value.len() > 40 {
        return Err("版本号无效。".into());
    }
    let parsed =
        Version::parse(value.strip_prefix('v').unwrap_or(value)).map_err(|_| "版本号无效。")?;
    if !parsed.pre.is_empty() || !parsed.build.is_empty() {
        return Err("更新源不是正式版本。".into());
    }
    Ok(parsed)
}

fn trusted_url(value: &str, path: &str) -> bool {
    Url::parse(value).ok().is_some_and(|url| {
        url.scheme() == "https"
            && url.host_str() == Some("github.com")
            && url.port().is_none()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none()
            && url.path().eq_ignore_ascii_case(path)
    })
}

fn trusted_redirect(url: &Url) -> bool {
    url.scheme() == "https"
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && matches!(
            url.host_str(),
            Some(
                "github.com"
                    | "release-assets.githubusercontent.com"
                    | "objects.githubusercontent.com"
            )
        )
}

fn client(download: bool) -> Result<Client, String> {
    Client::builder()
        .user_agent(concat!("SnowDown/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(35))
        .timeout(Duration::from_secs(if download { 900 } else { 25 }))
        .redirect(reqwest::redirect::Policy::custom(move |attempt| {
            if download && attempt.previous().len() < 6 && trusted_redirect(attempt.url()) {
                attempt.follow()
            } else {
                attempt.error("更新请求跳转到不支持的地址")
            }
        }))
        .build()
        .map_err(|_| "无法初始化更新连接。".into())
}

fn http_error(status: StatusCode) -> String {
    match status {
        StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS => {
            "GitHub 暂时限制请求频率，请稍后再检查。".into()
        }
        StatusCode::NOT_FOUND => "GitHub 上暂未找到发布版本，请稍后再检查。".into(),
        _ => format!("GitHub 更新服务暂不可用（HTTP {}）。", status.as_u16()),
    }
}

fn parse_release(value: &serde_json::Value, current: &str) -> Result<Option<ReleaseInfo>, String> {
    if value["draft"] != false || value["prerelease"] != false {
        return Err("更新源不是公开正式版本。".into());
    }
    let tag = value["tag_name"]
        .as_str()
        .ok_or("GitHub 版本信息不完整。")?;
    let latest = version(tag)?;
    if latest <= version(current)? {
        return Ok(None);
    }
    // Derive exact expected asset names and paths; never accept a caller-supplied installer URL.
    let file_name = format!("SnowDown_{latest}_x64-setup.exe");
    let asset = value["assets"]
        .as_array()
        .and_then(|assets| {
            assets
                .iter()
                .find(|asset| asset["name"] == file_name && asset["state"] == "uploaded")
        })
        .ok_or("新版暂未提供完整的 Windows x64 安装包，请稍后重试。")?;
    let size = asset["size"]
        .as_u64()
        .filter(|size| *size > 0 && *size <= MAX_INSTALLER)
        .ok_or("更新包大小无效。")?;
    let digest = asset["digest"]
        .as_str()
        .and_then(|digest| digest.strip_prefix("sha256:"))
        .filter(|digest| digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or("GitHub 暂未提供更新包校验信息，请稍后重试。")?;
    let download_url = asset["browser_download_url"]
        .as_str()
        .ok_or("更新包地址缺失。")?;
    if !trusted_url(
        download_url,
        &format!("/xiaogouyo/SnowDown/releases/download/{tag}/{file_name}"),
    ) {
        return Err("更新包地址不属于 SnowDown 官方仓库。".into());
    }
    Ok(Some(ReleaseInfo {
        version: latest.to_string(),
        title: value["name"]
            .as_str()
            .unwrap_or(tag)
            .chars()
            .take(160)
            .collect(),
        notes: value["body"]
            .as_str()
            .unwrap_or("")
            .chars()
            .take(32_000)
            .collect(),
        published_at: value["published_at"]
            .as_str()
            .unwrap_or("")
            .chars()
            .take(40)
            .collect(),
        url: format!("{RELEASES}/tag/{tag}"),
        file_name,
        size,
        download_url: download_url.into(),
        sha256: digest.to_ascii_lowercase(),
    }))
}

impl Updater {
    pub fn new(directory: PathBuf, current: &str) -> Self {
        let preferences = std::fs::read(directory.join("preferences.json"))
            .ok()
            .filter(|bytes| bytes.len() < 4096)
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self {
            directory,
            preferences: Mutex::new(preferences),
            status: Mutex::new(UpdateStatus {
                current_version: current.into(),
                release: None,
                phase: Phase::Idle,
                downloaded: 0,
                total: 0,
                error: None,
                deferred: false,
            }),
            operation: AsyncMutex::new(()),
            cancel: watch::channel(0).0,
        }
    }

    pub fn status(&self) -> UpdateStatus {
        let mut status = self.status.lock().unwrap().clone();
        let prefs = self.preferences.lock().unwrap();
        status.deferred = status.release.as_ref().is_some_and(|release| {
            prefs.version == release.version && prefs.remind_after > now_ms()
        });
        status
    }

    pub fn installing(&self) -> bool {
        self.status.lock().unwrap().phase == Phase::Installing
    }

    pub async fn check(&self) -> Result<UpdateStatus, String> {
        let _operation = self
            .operation
            .try_lock()
            .map_err(|_| "更新操作正在进行，请稍后再检查。")?;
        let mut response = client(false)?
            .get(LATEST)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2026-03-10")
            .send()
            .await
            .map_err(|_| "无法连接 GitHub，请检查网络后重试。")?;
        if !response.status().is_success() {
            return Err(http_error(response.status()));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| "读取 GitHub 版本信息失败，请重试。")?
        {
            if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
                return Err("GitHub 版本信息过大。".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        let value =
            serde_json::from_slice(&bytes).map_err(|_| "GitHub 返回的版本信息无法识别。")?;
        let current = self.status.lock().unwrap().current_version.clone();
        let release = parse_release(&value, &current)?;
        {
            let mut status = self.status.lock().unwrap();
            if status.release != release {
                status.phase = Phase::Idle;
                status.downloaded = 0;
                status.total = release.as_ref().map(|release| release.size).unwrap_or(0);
            }
            status.release = release;
            status.error = None;
        }
        Ok(self.status())
    }

    pub fn defer(&self) -> Result<UpdateStatus, String> {
        let release = self
            .status
            .lock()
            .unwrap()
            .release
            .clone()
            .ok_or("尚无可推迟的更新。")?;
        let prefs = Preferences {
            version: release.version,
            remind_after: now_ms().saturating_add(24 * 60 * 60 * 1000),
        };
        std::fs::create_dir_all(&self.directory).map_err(|_| "无法保存稍后提醒设置。")?;
        let temporary = self
            .directory
            .join(format!("prefs-{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| {
            std::fs::write(&temporary, serde_json::to_vec(&prefs).unwrap())?;
            std::fs::rename(&temporary, self.directory.join("preferences.json"))
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(temporary);
            return Err("无法保存稍后提醒设置，请检查文件权限。".into());
        }
        *self.preferences.lock().unwrap() = prefs;
        Ok(self.status())
    }

    pub fn cancel(&self) {
        if self.status.lock().unwrap().phase == Phase::Downloading {
            self.cancel
                .send_modify(|generation| *generation = generation.wrapping_add(1));
        }
    }

    fn progress(&self, phase: Phase, downloaded: u64, error: Option<String>) -> UpdateStatus {
        {
            let mut status = self.status.lock().unwrap();
            status.phase = phase;
            status.downloaded = downloaded;
            status.error = error;
        }
        self.status()
    }

    pub async fn download(&self, emit: impl Fn(UpdateStatus)) -> Result<UpdateStatus, String> {
        let _operation = self
            .operation
            .try_lock()
            .map_err(|_| "更新操作正在进行。")?;
        let release = self
            .status
            .lock()
            .unwrap()
            .release
            .clone()
            .ok_or("请先检查新版本。")?;
        let mut cancel = self.cancel.subscribe();
        emit(self.progress(Phase::Downloading, 0, None));
        let target = self.directory.join(&release.file_name);
        let part = self
            .directory
            .join(format!("download-{}.part", uuid::Uuid::new_v4()));
        let result = async {
            fs::create_dir_all(&self.directory).await.map_err(|_| "无法创建更新缓存目录。")?;
            if verify_file(&target, &release).await.is_ok() { return Ok(()) }
            let http = client(true)?;
            let mut response = tokio::select! {
                _ = cancel.changed() => return Err("已取消下载更新。".into()),
                result = http.get(&release.download_url).send() => result.map_err(|_| "无法下载更新包，请检查 GitHub 连接后重试。")?,
            };
            if !response.status().is_success() { return Err(http_error(response.status())) }
            if response.content_length().is_some_and(|length| length != release.size) { return Err("更新包大小与发布信息不一致。".into()) }
            let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&part).await.map_err(|_| "无法写入更新缓存。")?;
            let mut hasher = Sha256::new(); let mut received = 0; let mut last_emit = Instant::now();
            loop {
                let chunk = tokio::select! {
                    _ = cancel.changed() => return Err("已取消下载更新。".into()),
                    result = response.chunk() => result.map_err(|_| "更新下载中断，可以重试。")?,
                };
                let Some(chunk) = chunk else { break };
                received += chunk.len() as u64;
                if received > release.size { return Err("更新包超过预期大小。".into()) }
                hasher.update(&chunk);
                file.write_all(&chunk).await.map_err(|_| "无法保存更新包，请检查磁盘空间。")?;
                if last_emit.elapsed() >= Duration::from_millis(150) { emit(self.progress(Phase::Downloading, received, None)); last_emit = Instant::now(); }
            }
            if received != release.size || format!("{:x}", hasher.finalize()) != release.sha256 { return Err("更新包校验失败，请重新下载。".into()) }
            file.flush().await.map_err(|_| "无法保存更新包。")?;
            file.sync_all().await.map_err(|_| "无法保存更新包。")?;
            drop(file);
            if cancel.has_changed().unwrap_or(false) { return Err("已取消下载更新。".into()) }
            fs::rename(&part, &target).await.map_err(|_| "无法完成更新包保存，请关闭已打开的安装器后重试。")?;
            Ok::<(), String>(())
        }.await;
        let _ = fs::remove_file(&part).await;
        match result {
            Ok(()) => {
                let status = self.progress(Phase::Ready, release.size, None);
                emit(status.clone());
                Ok(status)
            }
            Err(error) => {
                let status =
                    self.progress(Phase::Error, self.status().downloaded, Some(error.clone()));
                emit(status);
                Err(error)
            }
        }
    }

    /// Re-read the full file immediately before installation. The UI never supplies a path.
    pub async fn verified_installer(&self) -> Result<PathBuf, String> {
        let status = self.status();
        if status.phase != Phase::Ready {
            return Err("请先下载并校验更新包。".into());
        }
        let release = status.release.ok_or("更新信息已失效，请重新检查。")?;
        let target = self.directory.join(&release.file_name);
        if let Err(error) = verify_file(&target, &release).await {
            self.progress(Phase::Error, 0, Some(error.clone()));
            return Err(error);
        }
        Ok(target)
    }

    pub fn mark_installing(&self) {
        self.status.lock().unwrap().phase = Phase::Installing;
    }
    pub fn install_failed(&self) {
        self.status.lock().unwrap().phase = Phase::Ready;
    }
}

async fn verify_file(path: &Path, release: &ReleaseInfo) -> Result<(), String> {
    let mut file = fs::File::open(path)
        .await
        .map_err(|_| "更新包不存在，请重新下载。")?;
    if file.metadata().await.map_err(|_| "无法读取更新包。")?.len() != release.size {
        return Err("更新包大小有变化，请重新下载。".into());
    }
    let mut hasher = Sha256::new();
    // Async futures are embedded in Tauri's Windows IPC dispatcher. Keep the hash
    // buffer on the heap so even unrelated commands fit the Windows thread stack.
    let mut buffer = vec![0u8; 128 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .await
            .map_err(|_| "无法读取更新包。")?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    if format!("{:x}", hasher.finalize()) != release.sha256 {
        return Err("更新包校验失败，请重新下载。".into());
    }
    Ok(())
}

pub fn releases_url() -> &'static str {
    RELEASES
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fixture() -> serde_json::Value {
        json!({"tag_name":"v0.2.0","name":"SnowDown v0.2.0","body":"Updates","draft":false,"prerelease":false,"published_at":"2026-10-24T00:00:00Z","assets":[{"name":"SnowDown_0.2.0_x64-setup.exe","state":"uploaded","size":3,"digest":format!("sha256:{:x}",Sha256::digest(b"abc")),"browser_download_url":"https://github.com/xiaogouyo/SnowDown/releases/download/v0.2.0/SnowDown_0.2.0_x64-setup.exe"}]})
    }
    fn temporary() -> PathBuf {
        std::env::temp_dir().join(format!("snowdown-updater-{}", uuid::Uuid::new_v4()))
    }
    fn seeded(directory: PathBuf) -> Updater {
        let updater = Updater::new(directory, "0.1.6");
        let release = parse_release(&fixture(), "0.1.6").unwrap();
        updater.status.lock().unwrap().total = 3;
        updater.status.lock().unwrap().release = release;
        updater
    }

    #[test]
    fn semantic_versions_prevent_downgrades_and_previews() {
        assert!(parse_release(&fixture(), "0.1.9").unwrap().is_some());
        assert!(parse_release(&fixture(), "0.2.0").unwrap().is_none());
        assert!(parse_release(&fixture(), "0.10.0").unwrap().is_none());
        for tag in ["v0.3.0-beta.1", "v0.3.0+build", "v../../exe", "v01.2.3"] {
            let mut value = fixture();
            value["tag_name"] = tag.into();
            assert!(parse_release(&value, "0.1.6").is_err());
        }
        let mut value = fixture();
        value["prerelease"] = true.into();
        assert!(parse_release(&value, "0.1.6").is_err());
        value = fixture();
        value["draft"] = true.into();
        assert!(parse_release(&value, "0.1.6").is_err());
    }

    #[test]
    fn release_requires_exact_repo_asset_size_and_digest() {
        for url in ["http://github.com/xiaogouyo/SnowDown/releases/download/v0.2.0/SnowDown_0.2.0_x64-setup.exe", "https://github.com.evil.test/xiaogouyo/SnowDown/releases/download/v0.2.0/SnowDown_0.2.0_x64-setup.exe", "https://github.com/another/SnowDown/releases/download/v0.2.0/SnowDown_0.2.0_x64-setup.exe", "https://user@github.com/xiaogouyo/SnowDown/releases/download/v0.2.0/SnowDown_0.2.0_x64-setup.exe"] {
            let mut value = fixture(); value["assets"][0]["browser_download_url"] = url.into(); assert!(parse_release(&value, "0.1.6").is_err());
        }
        for invalid in [
            json!(null),
            json!("sha256:abc"),
            json!(format!("md5:{}", "a".repeat(64))),
        ] {
            let mut value = fixture();
            value["assets"][0]["digest"] = invalid;
            assert!(parse_release(&value, "0.1.6").is_err());
        }
        let mut value = fixture();
        value["assets"][0]["size"] = (MAX_INSTALLER + 1).into();
        assert!(parse_release(&value, "0.1.6").is_err());
        value = fixture();
        value["assets"][0]["state"] = "new".into();
        assert!(parse_release(&value, "0.1.6").is_err());
        value = fixture();
        value["assets"][0]["name"] = "unexpected.exe".into();
        assert!(parse_release(&value, "0.1.6").is_err());
        let release = parse_release(&fixture(), "0.1.6").unwrap().unwrap();
        let public = serde_json::to_string(&release).unwrap();
        assert!(!public.contains("sha256"));
        assert!(!public.contains("download_url"));
    }

    #[test]
    fn redirects_are_limited_to_https_github_download_hosts() {
        for url in [
            "https://release-assets.githubusercontent.com/github-production-release-asset/1",
            "https://objects.githubusercontent.com/release/1",
        ] {
            assert!(trusted_redirect(&Url::parse(url).unwrap()));
        }
        for url in [
            "http://github.com/a",
            "https://github.com.evil.test/a",
            "https://example.com/a",
            "https://user@github.com/a",
            "https://github.com:8443/a",
        ] {
            assert!(!trusted_redirect(&Url::parse(url).unwrap()));
        }
    }

    #[test]
    fn update_futures_fit_the_windows_ipc_stack() {
        let updater = seeded(temporary());
        let download = updater.download(|_| {});
        let verify = updater.verified_installer();
        assert!(
            std::mem::size_of_val(&download) < 32 * 1024,
            "download future exceeds the IPC stack budget"
        );
        assert!(
            std::mem::size_of_val(&verify) < 32 * 1024,
            "installer verification future exceeds the IPC stack budget"
        );
    }

    #[test]
    fn postponement_survives_restart_and_is_specific_to_the_same_version() {
        let directory = temporary();
        let updater = seeded(directory.clone());
        assert!(updater.defer().unwrap().deferred);
        let restored = seeded(directory.clone());
        assert!(restored.status().deferred);
        restored
            .status
            .lock()
            .unwrap()
            .release
            .as_mut()
            .unwrap()
            .version = "0.3.0".into();
        assert!(!restored.status().deferred);
        restored.preferences.lock().unwrap().remind_after = 0;
        assert!(!restored.status().deferred);
        std::fs::remove_file(directory.join("preferences.json")).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }

    #[tokio::test]
    async fn cached_installers_are_verified_again_before_launch_and_operations_are_serialized() {
        let directory = temporary();
        fs::create_dir_all(&directory).await.unwrap();
        let updater = seeded(directory.clone());
        let target = directory.join("SnowDown_0.2.0_x64-setup.exe");
        fs::write(&target, b"abc").await.unwrap();
        let status = updater.download(|_| {}).await.unwrap();
        assert_eq!(status.phase, Phase::Ready);
        assert_eq!(updater.verified_installer().await.unwrap(), target);
        fs::write(&target, b"abd").await.unwrap();
        assert!(updater.verified_installer().await.is_err());
        let _guard = updater.operation.lock().await;
        assert!(updater.download(|_| {}).await.is_err());
        assert!(updater.check().await.is_err());
        drop(_guard);
        fs::remove_file(&target).await.unwrap();
        fs::remove_dir(directory).await.unwrap();
    }

    async fn server(body: &'static [u8], slow: bool) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/fixture", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut input = [0u8; 4096];
            let _ = socket.read(&mut input).await;
            let _ = socket
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    )
                    .as_bytes(),
                )
                .await;
            if slow {
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
            let _ = socket.write_all(body).await;
        });
        (url, task)
    }

    #[tokio::test]
    async fn transfers_reject_bad_hashes_remove_partial_files_and_can_be_cancelled() {
        let directory = temporary();
        let updater = seeded(directory.clone());
        let (url, server_task) = server(b"abd", false).await;
        updater
            .status
            .lock()
            .unwrap()
            .release
            .as_mut()
            .unwrap()
            .download_url = url;
        assert!(updater
            .download(|_| {})
            .await
            .unwrap_err()
            .contains("校验失败"));
        server_task.await.unwrap();
        assert_eq!(updater.status().phase, Phase::Error);
        assert!(std::fs::read_dir(&directory).unwrap().next().is_none());
        let (url, server_task) = server(b"abc", true).await;
        updater
            .status
            .lock()
            .unwrap()
            .release
            .as_mut()
            .unwrap()
            .download_url = url;
        let download = updater.download(|_| {});
        let cancel = async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            updater.cancel();
        };
        let (result, _) = tokio::join!(download, cancel);
        assert!(result.unwrap_err().contains("取消"));
        server_task.abort();
        assert!(std::fs::read_dir(&directory).unwrap().next().is_none());
        fs::remove_dir(directory).await.unwrap();
    }

    #[tokio::test]
    #[ignore = "Downloads the current public GitHub installer and verifies its hash without running it"]
    async fn live_github_release_and_verified_download() {
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.tools/updater-live-qa");
        let updater = Updater::new(directory.clone(), "0.0.0");
        let status = updater.check().await.unwrap();
        let release = status.release.unwrap();
        eprintln!(
            "Public GitHub latest version: {}, installer bytes: {}",
            release.version, release.size
        );
        let status = updater.download(|_| {}).await.unwrap();
        assert_eq!(status.phase, Phase::Ready);
        let path = updater.verified_installer().await.unwrap();
        assert!(fs::read(&path).await.unwrap().starts_with(b"MZ"));
        eprintln!("GitHub installer downloaded and verified; not executed.");
    }
}
