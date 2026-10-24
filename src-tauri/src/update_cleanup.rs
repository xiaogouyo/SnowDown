//! Only installer cache names owned by SnowDown are eligible; account, settings,
//! task and media directories are never traversed.
use semver::Version;
use std::path::Path;
use tokio::fs;

fn obsolete(name: &str, current: &Version) -> bool {
    let name = name.strip_suffix(".resume-v1").unwrap_or(name);
    let name = name.strip_suffix(".partial").unwrap_or(name);
    if let Some(value) = name
        .strip_prefix("SnowDown_")
        .and_then(|s| s.strip_suffix("_x64-setup.exe"))
    {
        return Version::parse(value)
            .ok()
            .is_some_and(|v| v.pre.is_empty() && v.build.is_empty() && v <= *current);
    }
    name.strip_prefix("download-")
        .and_then(|s| s.strip_suffix(".part"))
        .is_some_and(|id| uuid::Uuid::parse_str(id).is_ok())
}
fn reparse(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}
async fn cache(path: &Path) -> bool {
    let Ok(mut files) = fs::read_dir(path).await else {
        return false;
    };
    let mut owned = vec![];
    while let Ok(Some(entry)) = files.next_entry().await {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(metadata) = fs::symlink_metadata(entry.path()).await else {
            return false;
        };
        let block = name
            .strip_prefix("block-")
            .and_then(|s| s.strip_suffix(".json"))
            .is_some_and(|s| s.len() == 4 && s.bytes().all(|b| b.is_ascii_digit()));
        let checkpoint = name
            .strip_suffix(".tmp")
            .is_some_and(|s| uuid::Uuid::parse_str(s).is_ok());
        if !metadata.is_file()
            || reparse(&metadata)
            || !(block || checkpoint || matches!(name.as_str(), "manifest.json" | "data.part"))
        {
            return false;
        }
        owned.push(entry.path());
    }
    for file in owned {
        if fs::remove_file(file).await.is_err() {
            return false;
        }
    }
    fs::remove_dir(path).await.is_ok()
}
pub async fn installed(directory: &Path, version: &str) -> usize {
    let Ok(current) = Version::parse(version) else {
        return 0;
    };
    let Ok(metadata) = fs::symlink_metadata(directory).await else {
        return 0;
    };
    if reparse(&metadata) || !metadata.is_dir() {
        return 0;
    }
    let Ok(mut files) = fs::read_dir(directory).await else {
        return 0;
    };
    let mut removed = 0;
    while let Ok(Some(entry)) = files.next_entry().await {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !obsolete(&name, &current) {
            continue;
        }
        let Ok(metadata) = fs::symlink_metadata(entry.path()).await else {
            continue;
        };
        if reparse(&metadata) {
            continue;
        }
        if metadata.is_file() && fs::remove_file(entry.path()).await.is_ok()
            || metadata.is_dir()
                && name.ends_with(".partial.resume-v1")
                && cache(&entry.path()).await
        {
            removed += 1;
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn cleanup_preserves_all_user_data_future_updates_and_unrelated_files() {
        let dir = std::env::temp_dir().join(format!("snowdown-cleanup-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&dir).await.unwrap();
        let keep = [
            "snowdown.db",
            "preferences.json",
            "Cookies",
            "site-sessions.dat",
            "video.mp4",
            "SnowDown_0.1.12_x64-setup.exe",
            "SnowDown_0.1.12_x64-setup.exe.partial",
            "my-installer.exe",
        ];
        for name in keep {
            fs::write(dir.join(name), b"preserve").await.unwrap();
        }
        for name in [
            "SnowDown_0.1.10_x64-setup.exe",
            "SnowDown_0.1.11_x64-setup.exe",
            "SnowDown_0.1.10_x64-setup.exe.partial",
        ] {
            fs::write(dir.join(name), b"old").await.unwrap();
        }
        let part = dir.join("SnowDown_0.1.10_x64-setup.exe.partial.resume-v1");
        fs::create_dir(&part).await.unwrap();
        fs::write(part.join("data.part"), b"old").await.unwrap();
        assert_eq!(installed(&dir, "0.1.11").await, 4);
        for name in keep {
            assert_eq!(fs::read(dir.join(name)).await.unwrap(), b"preserve");
        }
        let guarded = dir.join("SnowDown_0.1.9_x64-setup.exe.partial.resume-v1");
        fs::create_dir(&guarded).await.unwrap();
        fs::write(guarded.join("Cookies"), b"keep").await.unwrap();
        installed(&dir, "0.1.11").await;
        assert!(guarded.join("Cookies").exists());
        fs::remove_dir_all(dir).await.unwrap();
    }
}
