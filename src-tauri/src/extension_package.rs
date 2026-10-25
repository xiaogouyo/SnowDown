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
    use std::os::windows::process::CommandExt;
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
    // Paths are passed as environment values, never interpolated into shell code.
    let result = std::process::Command::new("powershell.exe")
        .args(["-NoProfile","-NonInteractive","-WindowStyle","Hidden","-Command",
            "$ErrorActionPreference='Stop'; Add-Type -AssemblyName System.IO.Compression.FileSystem; [IO.Compression.ZipFile]::CreateFromDirectory($env:SNOWDOWN_ZIP_SOURCE,$env:SNOWDOWN_ZIP_TARGET,[IO.Compression.CompressionLevel]::Optimal,$false)"])
        .env("SNOWDOWN_ZIP_SOURCE", &temporary.directory)
        .env("SNOWDOWN_ZIP_TARGET", &temporary.archive)
        .creation_flags(0x08000000).output().map_err(|_|"无法创建插件 ZIP，请检查本机 Windows 组件。")?;
    if !result.status.success() || !fs::metadata(&temporary.archive).is_ok_and(|m| m.len() > 22) {
        return Err("无法生成插件 ZIP，请检查保存目录是否可写。".into());
    }
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
    use std::os::windows::process::CommandExt;
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
        let read=std::process::Command::new("powershell.exe").args(["-NoProfile","-NonInteractive","-Command",
            "Add-Type -AssemblyName System.IO.Compression.FileSystem; $z=[IO.Compression.ZipFile]::OpenRead($env:SNOWDOWN_TEST_ZIP); try { @($z.Entries | ForEach-Object { $_.FullName.Replace('\\','/') }) | ConvertTo-Json -Compress } finally { $z.Dispose() }"])
            .env("SNOWDOWN_TEST_ZIP",&target).creation_flags(0x08000000).output().unwrap();
        assert!(read.status.success());
        let mut entries: Vec<String> = serde_json::from_slice(&read.stdout).unwrap();
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
