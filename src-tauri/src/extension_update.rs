use reqwest::{Client, StatusCode};
use semver::Version;
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use url::Url;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    pub version: String,
    pub title: String,
    pub notes: String,
    pub url: String,
    pub file_name: String,
    pub size: u64,
    pub published_at: String,
    #[serde(skip)]
    download_url: String,
    #[serde(skip)]
    sha256: String,
}
fn stable(input: &str) -> Option<Version> {
    let version = Version::parse(input).ok()?;
    if input.len() > 32 || !version.pre.is_empty() || !version.build.is_empty() {
        None
    } else {
        Some(version)
    }
}
fn tag(input: &str) -> bool {
    input
        .strip_prefix("extension-v")
        .or_else(|| input.strip_prefix('v'))
        .and_then(stable)
        .is_some()
}
fn official(url: &Url) -> bool {
    url.scheme() == "https"
        && url.host_str() == Some("github.com")
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
}
fn repo(path: &str) -> Option<&str> {
    let parts: Vec<_> = path.split('/').collect();
    if parts.len() < 4
        || !parts[1].eq_ignore_ascii_case("xiaogouyo")
        || !parts[2].eq_ignore_ascii_case("snowdown")
    {
        None
    } else {
        Some(parts[3])
    }
}
pub fn release_url(input: &str) -> bool {
    Url::parse(input).ok().is_some_and(|url| {
        let parts: Vec<_> = url.path().split('/').collect();
        official(&url)
            && repo(url.path()) == Some("releases")
            && parts.len() == 6
            && parts[4] == "tag"
            && tag(parts[5])
    })
}
fn download_url(input: &str, name: &str) -> bool {
    Url::parse(input).ok().is_some_and(|url| {
        let parts: Vec<_> = url.path().split('/').collect();
        official(&url)
            && repo(url.path()) == Some("releases")
            && parts.len() == 7
            && parts[4] == "download"
            && tag(parts[5])
            && parts[6] == name
    })
}
fn select(value: &Value) -> Result<Release, String> {
    let rows = value
        .as_array()
        .ok_or("GitHub 返回了无效的插件发布数据。")?;
    let mut candidates = vec![];
    for row in rows {
        if row["draft"] != false
            || row["prerelease"] != false
            || !row["tag_name"].as_str().is_some_and(tag)
        {
            continue;
        }
        let Some(url) = row["html_url"].as_str().filter(|url| release_url(url)) else {
            continue;
        };
        for asset in row["assets"].as_array().into_iter().flatten() {
            let Some(name) = asset["name"].as_str() else {
                continue;
            };
            let Some(version) = name
                .strip_prefix("SnowDown_BrowserExtension_")
                .and_then(|s| s.strip_suffix(".zip"))
                .and_then(stable)
            else {
                continue;
            };
            let size = asset["size"].as_u64().unwrap_or(0);
            let Some(download) = asset["browser_download_url"]
                .as_str()
                .filter(|url| download_url(url, name))
            else {
                continue;
            };
            let Some(hash) = asset["digest"]
                .as_str()
                .and_then(|s| s.strip_prefix("sha256:"))
                .filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
            else {
                continue;
            };
            if asset["state"] != "uploaded" || size == 0 || size > 16 * 1024 * 1024 {
                continue;
            }
            candidates.push((
                version.clone(),
                Release {
                    version: version.to_string(),
                    title: row["name"]
                        .as_str()
                        .unwrap_or("插件更新")
                        .chars()
                        .take(200)
                        .collect(),
                    notes: row["body"]
                        .as_str()
                        .unwrap_or("")
                        .chars()
                        .take(6000)
                        .collect(),
                    url: url.into(),
                    file_name: name.into(),
                    size,
                    published_at: row["published_at"].as_str().unwrap_or("").into(),
                    download_url: download.into(),
                    sha256: hash.to_lowercase(),
                },
            ));
        }
    }
    candidates.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then(
                b.1.url
                    .contains("/extension-v")
                    .cmp(&a.1.url.contains("/extension-v")),
            )
            .then(b.1.published_at.cmp(&a.1.published_at))
    });
    candidates
        .into_iter()
        .next()
        .map(|(_, release)| release)
        .ok_or("暂未找到带校验信息的正式插件安装包。".into())
}
fn client(download: bool) -> Result<Client, String> {
    let mut builder = Client::builder();
    if let Some(proxy) = crate::extractor_proxy::upstream() {
        builder =
            builder.proxy(reqwest::Proxy::all(proxy.as_str()).map_err(|_| "系统代理配置无效。")?);
    }
    builder
        .user_agent(concat!("SnowDown/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(6))
        .timeout(Duration::from_secs(if download { 60 } else { 20 }))
        .redirect(reqwest::redirect::Policy::custom(move |attempt| {
            let url = attempt.url();
            if download
                && attempt.previous().len() < 5
                && url.scheme() == "https"
                && url.username().is_empty()
                && url.password().is_none()
                && url.port().is_none()
                && matches!(
                    url.host_str(),
                    Some(
                        "github.com"
                            | "release-assets.githubusercontent.com"
                            | "objects.githubusercontent.com"
                            | "github-releases.githubusercontent.com"
                    )
                )
            {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .build()
        .map_err(|_| "无法创建插件更新连接。".into())
}
async fn bounded(mut response: reqwest::Response, limit: usize) -> Result<Vec<u8>, String> {
    if response.content_length().is_some_and(|n| n > limit as u64) {
        return Err("插件更新数据过大。".into());
    }
    let mut bytes = vec![];
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "插件更新传输中断，请重试。")?
    {
        if bytes.len() + chunk.len() > limit {
            return Err("插件更新数据过大。".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
pub async fn check() -> Result<Release, String> {
    let response = client(false)?
        .get("https://api.github.com/repos/xiaogouyo/SnowDown/releases?per_page=100")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|_| "无法连接 GitHub 检查插件更新，请检查网络后重试。")?;
    if response.status() == StatusCode::FORBIDDEN
        || response.status() == StatusCode::TOO_MANY_REQUESTS
    {
        return Err("GitHub 暂时限制更新查询，请稍后重试。".into());
    }
    if !response.status().is_success() {
        return Err(format!(
            "插件更新检查失败（HTTP {}）。",
            response.status().as_u16()
        ));
    }
    let value = serde_json::from_slice(&bounded(response, 4 * 1024 * 1024).await?)
        .map_err(|_| "GitHub 插件发布数据无法读取。")?;
    select(&value)
}
struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
pub async fn download(path: &str, version: &str) -> Result<String, String> {
    let target = Path::new(path);
    if !target.is_absolute()
        || !target
            .extension()
            .is_some_and(|e| e.to_str().is_some_and(|s| s.eq_ignore_ascii_case("zip")))
    {
        return Err("请选择 ZIP 文件保存位置。".into());
    }
    let parent = target
        .parent()
        .filter(|p| p.is_dir())
        .ok_or("保存目录不存在。")?;
    let release = check().await?;
    if release.version != version {
        return Err("插件最新版本已变化，请重新检查更新。".into());
    }
    let response = client(true)?
        .get(&release.download_url)
        .send()
        .await
        .map_err(|_| "无法下载插件更新，请重试或打开发布页面。")?;
    if !response.status().is_success() {
        return Err(format!(
            "插件下载失败（HTTP {}）。",
            response.status().as_u16()
        ));
    }
    let bytes = bounded(response, 16 * 1024 * 1024).await?;
    if bytes.len() as u64 != release.size
        || format!("{:x}", Sha256::digest(&bytes)) != release.sha256
    {
        return Err("插件安装包校验失败，未保存文件，请重新下载。".into());
    }
    let temporary =
        Temporary(parent.join(format!(".snowdown-plugin-{}.zip", uuid::Uuid::new_v4())));
    tokio::fs::write(&temporary.0, bytes)
        .await
        .map_err(|_| "无法写入插件安装包。")?;
    tokio::fs::rename(&temporary.0, target)
        .await
        .map_err(|_| "无法保存插件安装包，请关闭占用文件后重试。")?;
    Ok(path.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn item(version: &str) -> Value {
        json!({"tag_name":format!("extension-v{version}"),"draft":false,"prerelease":false,"name":"plugin","body":"notes","html_url":format!("https://github.com/xiaogouyo/SnowDown/releases/tag/extension-v{version}"),"assets":[{"name":format!("SnowDown_BrowserExtension_{version}.zip"),"size":12,"state":"uploaded","digest":format!("sha256:{}","a".repeat(64)),"browser_download_url":format!("https://github.com/xiaogouyo/SnowDown/releases/download/extension-v{version}/SnowDown_BrowserExtension_{version}.zip")} ]})
    }
    #[test]
    fn chooses_semver_not_desktop_latest_or_list_order() {
        assert_eq!(
            select(&json!([item("0.1.8"), item("0.1.10"), item("0.1.9")]))
                .unwrap()
                .version,
            "0.1.10"
        );
        let mut desktop = item("0.1.9");
        desktop["tag_name"] = json!("v0.2.10");
        assert_eq!(select(&json!([desktop])).unwrap().version, "0.1.9")
    }
    #[test]
    fn rejects_previews_wrong_repository_and_unverified_assets() {
        for key in ["draft", "prerelease"] {
            let mut row = item("0.1.9");
            row[key] = json!(true);
            assert!(select(&json!([row])).is_err())
        }
        for value in [
            "https://github.com.evil.test/xiaogouyo/SnowDown/releases/tag/extension-v0.1.9",
            "https://github.com/other/SnowDown/releases/tag/extension-v0.1.9",
        ] {
            let mut row = item("0.1.9");
            row["html_url"] = json!(value);
            assert!(select(&json!([row])).is_err())
        }
        let mut row = item("0.1.9");
        row["assets"][0]["digest"] = json!(null);
        assert!(select(&json!([row])).is_err());
        let mut row = item("0.1.9");
        row["assets"][0]["browser_download_url"] = json!("https://evil.test/plugin.zip");
        assert!(select(&json!([row])).is_err())
    }
}
