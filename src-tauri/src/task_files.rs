//! Only incomplete files owned by a removed UUID task are eligible for cleanup.
use std::path::PathBuf;
pub async fn cleanup(directory: String, id: String) {
    if uuid::Uuid::parse_str(&id).is_err() {
        return;
    }
    let root = PathBuf::from(directory);
    let cache = root.join(".snowdown");
    let target = cache.join(&id);
    if target.parent() != Some(cache.as_path()) {
        return;
    }
    for path in [&root, &cache, &target] {
        let Ok(meta) = tokio::fs::symlink_metadata(path).await else {
            return;
        };
        if !meta.is_dir() || meta.is_symlink() {
            return;
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if meta.file_attributes() & 0x400 != 0 {
                return;
            }
        }
    }
    let _ = tokio::fs::remove_dir_all(target).await;
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn removal_only_cleans_the_owned_task_cache() {
        let root = std::env::temp_dir().join(format!("snowdown-remove-{}", uuid::Uuid::new_v4()));
        let id = uuid::Uuid::new_v4().to_string();
        let cache = root.join(".snowdown").join(&id);
        tokio::fs::create_dir_all(&cache).await.unwrap();
        tokio::fs::write(cache.join("data.part"), b"partial")
            .await
            .unwrap();
        tokio::fs::write(root.join("video.mp4"), b"keep")
            .await
            .unwrap();
        let other = root
            .join(".snowdown")
            .join(uuid::Uuid::new_v4().to_string());
        tokio::fs::create_dir(&other).await.unwrap();
        super::cleanup(root.to_string_lossy().into(), "../video.mp4".into()).await;
        assert!(cache.exists());
        super::cleanup(root.to_string_lossy().into(), id).await;
        assert!(!cache.exists());
        assert!(other.exists());
        assert_eq!(
            tokio::fs::read(root.join("video.mp4")).await.unwrap(),
            b"keep"
        );
        tokio::fs::remove_dir_all(root).await.unwrap();
    }
}
