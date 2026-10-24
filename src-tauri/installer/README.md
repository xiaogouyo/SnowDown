# SnowDown Windows installer

`snowdown.nsi` is a local customization of Tauri's NSIS template, retrieved on 2026-10-24 from:

https://github.com/tauri-apps/tauri/blob/dev/crates/tauri-bundler/src/bundle/windows/nsis/installer.nsi

It retains Tauri's upgrade detection, WebView2 bootstrap, shortcut registration and uninstaller. The public installer now uses the shaped WPF frontend in the root `installer/` directory; this NSIS executable is its internal silent payload. It reports real installation phases and refuses to force-close an active client when invoked by that frontend. Standalone NSIS fallback pages keep a white/blue theme; `sidebar.bmp` is a locally drawn SnowDown finish-page graphic.

When updating Tauri, compare the upstream template and review any installation changes before replacing this file. Tauri is copyright Tauri Programme within The Commons Conservancy, MIT / Apache-2.0. See upstream [LICENSE-MIT](https://github.com/tauri-apps/tauri/blob/dev/LICENSE-MIT) and [LICENSE-APACHE-2.0](https://github.com/tauri-apps/tauri/blob/dev/LICENSE-APACHE-2.0).
