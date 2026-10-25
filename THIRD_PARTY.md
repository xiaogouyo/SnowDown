# Third-party components

SnowDown uses Vue (MIT), Tauri (MIT / Apache-2.0), Rust libraries under their individual licenses, SQLite (public domain), and Lucide icons (ISC). Dependency versions are pinned by package-lock.json and src-tauri/Cargo.lock.

The browser extension uses [Mediabunny](https://github.com/Vanilagy/mediabunny) (MPL-2.0) to merge separate MP4 video/audio in the browser. Its unmodified source version is pinned in package-lock.json and its full license is bundled in the extension. Snowflake artwork derives from the existing Lucide icon system (ISC); the extension contains only a public key for a stable development ID.

The QR code renderer uses the qrcode Rust crate (MIT / Apache-2.0). The customized NSIS installer template is derived from [Tauri's installer template](https://github.com/tauri-apps/tauri/blob/dev/crates/tauri-bundler/src/bundle/windows/nsis/installer.nsi), under MIT / Apache-2.0, copyright Tauri Programme within The Commons Conservancy. The source template is included in `src-tauri/installer/snowdown.nsi`.

The Windows background parser uses webview2-com (MIT) and windows-rs (MIT / Apache-2.0) to observe video-detail responses in the app's own hidden WebView2 window, sharing only the app's platform login profile. Source and dependency versions are recorded in Cargo.lock.

The Windows runtime package contains the separately executed FFmpeg and ffprobe binaries. This build uses FFmpeg 9.0.1 essentials from Gyan.dev, licensed under GPL version 3 or later. Its full license is bundled as resources/FFMPEG-LICENSE.txt. Build provenance is recorded in resources/ffmpeg-build.json.

- Build provider and source/build instructions: https://www.gyan.dev/ffmpeg/builds/
- FFmpeg source repository: https://git.ffmpeg.org/ffmpeg.git
- FFmpeg source release: https://ffmpeg.org/releases/ffmpeg-9.0.1.tar.xz
- FFmpeg license: https://ffmpeg.org/legal.html

The media setup script can also retrieve yt-dlp/FFmpeg-Builds binaries; these have their own matching build provenance and license. Preserve the bundled license and obtain matching source/build information when redistributing a different build.

Multi-site compatibility uses separately executed, unmodified [yt-dlp](https://github.com/yt-dlp/yt-dlp) and [Deno](https://github.com/denoland/deno) binaries. The yt-dlp repository is Unlicense; its Windows PyInstaller executable is a GPL-3.0-or-later combined distribution. Deno is MIT. Corresponding versions, official asset/source URLs and SHA256 values are pinned in `resources/extractor-build.json`; notices and full upstream license texts are bundled as `resources/EXTRACTOR-LICENSE.txt`. Setup retrieves only those official releases, verifies their published digests and does not enable external plugins or remote components at runtime. Matching source distributions are available under each pinned upstream release tag.
