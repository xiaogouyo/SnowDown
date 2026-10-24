use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub download_dir: String,
    pub concurrency: usize,
    pub max_speed_mbps: u32,
    pub save_cover: bool,
    pub save_danmaku: bool,
    pub save_subtitles: bool,
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
    pub error: Option<String>,
    pub output_path: Option<String>,
    pub created_at: u64,
    pub download_dir: String,
    pub save_cover: bool,
    pub save_danmaku: bool,
    pub save_subtitles: bool,
    pub warning: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    pub tasks: Vec<DownloadTask>,
    pub settings: Settings,
    pub media_ready: bool,
    pub authenticated: bool,
    pub auth: crate::auth::AuthStatus,
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
    fn filenames_are_windows_safe() {
        assert_eq!(safe_filename("试验: A/B?*... "), "试验_ A_B__");
        assert_eq!(safe_filename("CON.txt"), "_CON.txt");
        assert_eq!(safe_filename("..."), "video");
        assert!(safe_filename(&"好".repeat(200)).chars().count() <= 95);
    }
}
