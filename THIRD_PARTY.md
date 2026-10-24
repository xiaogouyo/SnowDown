# Third-party components

SnowDown uses Vue (MIT), Tauri (MIT / Apache-2.0), Rust libraries under their individual licenses, SQLite (public domain), and Lucide icons (ISC). Dependency versions are pinned by package-lock.json and src-tauri/Cargo.lock.

The QR code renderer uses the qrcode Rust crate (MIT / Apache-2.0). The customized NSIS installer template is derived from [Tauri's installer template](https://github.com/tauri-apps/tauri/blob/dev/crates/tauri-bundler/src/bundle/windows/nsis/installer.nsi), under MIT / Apache-2.0, copyright Tauri Programme within The Commons Conservancy. The source template is included in `src-tauri/installer/snowdown.nsi`.

The Windows runtime package contains the separately executed FFmpeg binary; ffprobe is used for development verification. This build uses FFmpeg 9.0.1 essentials from Gyan.dev, licensed under GPL version 3 or later. Its full license is bundled as resources/FFMPEG-LICENSE.txt. Build provenance is recorded in resources/ffmpeg-build.json.

- Build provider and source/build instructions: https://www.gyan.dev/ffmpeg/builds/
- FFmpeg source repository: https://git.ffmpeg.org/ffmpeg.git
- FFmpeg source release: https://ffmpeg.org/releases/ffmpeg-9.0.1.tar.xz
- FFmpeg license: https://ffmpeg.org/legal.html

The media setup script can also retrieve yt-dlp/FFmpeg-Builds binaries; these have their own matching build provenance and license. Preserve the bundled license and obtain matching source/build information when redistributing a different build.
