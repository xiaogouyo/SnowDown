use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    #[default]
    Bilibili,
    Douyin,
    Xiaohongshu,
    Channels,
    Web,
}

impl Platform {
    pub fn key(self) -> &'static str {
        match self {
            Self::Bilibili => "bilibili",
            Self::Douyin => "douyin",
            Self::Xiaohongshu => "xiaohongshu",
            Self::Channels => "channels",
            Self::Web => "web",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Bilibili => "B站",
            Self::Douyin => "抖音",
            Self::Xiaohongshu => "小红书",
            Self::Channels => "视频号",
            Self::Web => "网页视频",
        }
    }
    pub fn home(self) -> &'static str {
        match self {
            Self::Bilibili => "https://www.bilibili.com/",
            Self::Douyin => "https://www.douyin.com/",
            Self::Xiaohongshu => "https://www.xiaohongshu.com/",
            Self::Channels => "https://channels.weixin.qq.com/",
            Self::Web => "https://www.youtube.com/",
        }
    }
}

fn enabled() -> bool {
    true
}
fn default_connections() -> usize {
    8
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub download_dir: String,
    pub concurrency: usize,
    pub max_speed_mbps: u32,
    #[serde(default = "default_connections")]
    pub download_connections: usize,
    pub save_cover: bool,
    pub save_danmaku: bool,
    pub save_subtitles: bool,
    #[serde(default = "enabled")]
    pub clipboard_enabled: bool,
    #[serde(default)]
    pub clipboard_auto_parse: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct VideoPage {
    pub cid: u64,
    pub page: u32,
    pub part: String,
    pub duration: u64,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Quality {
    pub id: u32,
    pub label: String,
    pub width: u64,
    pub height: u64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoInfo {
    #[serde(default)]
    pub platform: Platform,
    #[serde(default)]
    pub source_url: String,
    pub bvid: String,
    pub title: String,
    pub cover: String,
    pub author: String,
    pub duration: u64,
    pub pages: Vec<VideoPage>,
    pub qualities: Vec<Quality>,
    pub selected_cid: u64,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Queued,
    Downloading,
    Merging,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    Video,
    Audio,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadTask {
    #[serde(default)]
    pub resource: Option<ResourceSpec>,
    #[serde(default)]
    pub platform: Platform,
    #[serde(default)]
    pub source_url: String,
    pub id: String,
    pub bvid: String,
    pub cid: u64,
    pub title: String,
    pub page_title: String,
    pub page: u32,
    pub cover: String,
    pub quality: u32,
    pub quality_label: String,
    pub format: OutputFormat,
    pub status: TaskStatus,
    pub progress: f64,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub speed: u64,
    #[serde(default)]
    pub active_connections: usize,
    #[serde(default)]
    pub peak_connections: usize,
    #[serde(default)]
    pub resumed_bytes: u64,
    #[serde(default)]
    pub range_download: bool,
    pub error: Option<String>,
    pub output_path: Option<String>,
    pub created_at: u64,
    pub download_dir: String,
    pub save_cover: bool,
    pub save_danmaku: bool,
    pub save_subtitles: bool,
    pub warning: Option<String>,
    #[serde(default)]
    pub batch_id: Option<String>,
    #[serde(default)]
    pub start_interval_secs: u32,
    #[serde(default)]
    pub not_before: u64,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct ResourceSpec {
    pub url: String,
    pub extension: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub archive: Vec<ArchiveResource>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ArchiveResource {
    pub url: String,
    pub name: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    pub tasks: Vec<DownloadTask>,
    pub settings: Settings,
    pub media_ready: bool,
    pub authenticated: bool,
    pub auth: crate::auth::AuthStatus,
    pub site_sessions: Vec<crate::site_sessions::SessionStatus>,
    pub version: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTaskRequest {
    pub video: VideoInfo,
    pub page_cids: Vec<u64>,
    pub quality: u32,
    pub format: OutputFormat,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchOptions {
    pub download_dir: String,
    pub interval_secs: u32,
    pub save_cover: bool,
    pub save_danmaku: bool,
    pub save_subtitles: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateBatchRequest {
    pub items: Vec<CreateTaskRequest>,
    pub options: BatchOptions,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchResult {
    pub added: usize,
    pub skipped: usize,
}

pub fn safe_filename(input: &str) -> String {
    let cleaned: String = input
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .take(95)
        .collect();
    let cleaned = cleaned.trim_matches(|c: char| c.is_whitespace() || c == '.');
    if cleaned.is_empty() {
        return "video".into();
    }
    let stem = cleaned.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit());
    if reserved {
        format!("_{cleaned}")
    } else {
        cleaned.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_settings_and_video_records_keep_bilibili_defaults() {
        let settings: Settings = serde_json::from_value(serde_json::json!({"downloadDir":"D:\\videos","concurrency":2,"maxSpeedMbps":0,"saveCover":false,"saveDanmaku":false,"saveSubtitles":false})).unwrap();
        assert!(settings.clipboard_enabled);
        assert!(!settings.clipboard_auto_parse);
        assert_eq!(settings.download_connections, 8);
        let video: VideoInfo = serde_json::from_value(serde_json::json!({"bvid":"BV1xx411c7mD","title":"legacy","cover":"","author":"","duration":1,"pages":[],"qualities":[],"selectedCid":1})).unwrap();
        assert_eq!(video.platform, Platform::Bilibili);
        assert!(video.source_url.is_empty());
    }
    #[test]
    fn filenames_are_windows_safe() {
        assert_eq!(safe_filename("试验: A/B?*... "), "试验_ A_B__");
        assert_eq!(safe_filename("CON.txt"), "_CON.txt");
        assert_eq!(safe_filename("..."), "video");
        assert!(safe_filename(&"好".repeat(200)).chars().count() <= 95);
    }
}
