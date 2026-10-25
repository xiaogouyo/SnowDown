//! Offline export of the public, bundled extension. No browser profile or
//! application state is read into the archive.
use std::{
    fs,
    path::{Path, PathBuf},
};

const FILES: &[&str] = &[
    "manifest.json",
    "popup.html",
    "job.html",
    "style.css",
    "page-hook.js",
    "background.js",
    "popup.js",
    "job.js",
    "floating.js",
    "preview.js",
    "MEDIABUNNY-LICENSE.txt",
    "icons/32.png",
    "icons/128.png",
];
struct Temporary {
    directory: PathBuf,
    archive: PathBuf,
}
impl Drop for Temporary {
    fn drop(&mut self) {
        // Both paths are generated with a UUID and created by this operation.
        let _ = fs::remove_file(&self.archive);
        let _ = fs::remove_dir_all(&self.directory);
    }
}

#[cfg(windows)]
pub fn export(source: &Path, destination: &Path) -> Result<PathBuf, String> {
    let mut target = destination.to_owned();
    if target.extension().is_none() {
        target.set_extension("zip");
    }
    if !target.is_absolute()
        || !target
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("zip"))
        || target.is_dir()
    {
        return Err("请选择 ZIP 文件的保存位置。".into());
    }
    let parent = target
        .parent()
        .filter(|p| p.is_dir())
        .ok_or("保存目录不存在。")?;
    let id = uuid::Uuid::new_v4();
    let directory = std::env::temp_dir().join(format!("snowdown-extension-{id}"));
    fs::create_dir(&directory).map_err(|_| "无法创建插件临时目录。")?;
    let temporary = Temporary {
        directory,
        archive: parent.join(format!(".snowdown-extension-{id}.zip")),
    };
    fs::create_dir(temporary.directory.join("icons")).map_err(|_| "无法准备插件图标。")?;
    let mut total = 0;
    for file in FILES {
        let origin = source.join(file);
        let metadata =
            fs::symlink_metadata(&origin).map_err(|_| "插件文件不完整，请重新安装完整软件。")?;
        total += metadata.len();
        if !metadata.file_type().is_file()
            || metadata.len() > 4 * 1024 * 1024
            || total > 16 * 1024 * 1024
        {
            return Err("插件资源无效，请重新安装完整软件。".into());
        }
        fs::copy(origin, temporary.directory.join(file)).map_err(|_| "无法读取内置插件文件。")?;
    }
    // Native ZIP creation avoids invoking a shell or depending on PowerShell's
    // runtime/module configuration. Only the explicit public bundle is included.
    let archive = fs::File::create(&temporary.archive).map_err(|_| "无法创建插件 ZIP。")?;
    let mut writer = zip::ZipWriter::new(archive);
    for file in FILES {
        writer
            .start_file(
                *file,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .map_err(|_| "无法生成插件 ZIP。")?;
        let mut source =
            fs::File::open(temporary.directory.join(file)).map_err(|_| "无法读取插件文件。")?;
        std::io::copy(&mut source, &mut writer).map_err(|_| "无法写入插件 ZIP。")?;
    }
    writer
        .finish()
        .map_err(|_| "无法完成插件 ZIP。")?
        .sync_all()
        .map_err(|_| "无法保存插件 ZIP。")?;
    // Build beside the destination, then replace it only after ZIP creation
    // succeeds. A missing resource or failed archive leaves an existing ZIP intact.
    fs::rename(&temporary.archive, &target)
        .map_err(|_| "无法保存插件 ZIP，请关闭占用该文件的程序后重试。")?;
    Ok(target)
}
#[cfg(not(windows))]
pub fn export(_: &Path, _: &Path) -> Result<PathBuf, String> {
    Err("此安装包导出功能目前用于 Windows 桌面版。".into())
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[test]
    fn exports_only_public_bundle_handles_literal_paths_and_keeps_existing_zip_on_failure() {
        let directory =
            std::env::temp_dir().join(format!("snowdown-package-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        let source = directory.join("source");
        fs::create_dir(&source).unwrap();
        fs::create_dir(source.join("icons")).unwrap();
        let bundle = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/browser-extension");
        for file in FILES {
            fs::copy(bundle.join(file), source.join(file)).unwrap();
        }
        fs::write(source.join("Cookies"), "never-export-account-data").unwrap();
        fs::write(source.join("bindings.json"), "never-export-binding-data").unwrap();
        let parent = directory.join("本地 空格 ' $目录");
        fs::create_dir(&parent).unwrap();
        let target = parent.join("SnowDown.zip");
        assert_eq!(export(&source, &target).unwrap(), target);
        let mut archive = zip::ZipArchive::new(fs::File::open(&target).unwrap()).unwrap();
        let mut entries: Vec<String> = archive.file_names().map(str::to_owned).collect();
        for file in FILES {
            use std::io::Read;
            let mut entry = archive.by_name(file).unwrap();
            let mut bytes = vec![];
            entry.read_to_end(&mut bytes).unwrap();
            assert_eq!(bytes, fs::read(source.join(file)).unwrap());
        }
        drop(archive);
        entries.sort();
        let mut expected: Vec<_> = FILES.iter().map(|x| x.to_string()).collect();
        expected.sort();
        assert_eq!(entries, expected);
        // Successful overwrite and a failed subsequent export both preserve a valid file.
        export(&source, &target).unwrap();
        let previous = fs::read(&target).unwrap();
        fs::remove_file(source.join("manifest.json")).unwrap();
        assert!(export(&source, &target).is_err());
        assert_eq!(fs::read(&target).unwrap(), previous);
        assert_eq!(fs::read_dir(&parent).unwrap().count(), 1);
        assert!(export(&source, &parent.join("Cookies")).is_err());
        fs::remove_dir_all(directory).unwrap();
    }
}
