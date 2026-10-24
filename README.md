# SnowDown

Windows 桌面端B站视频下载工具。采用 Tauri 2、Vue 3、TypeScript 与 Rust，下载文件在本机处理和保存。

## 下载 v0.1.0

- [Windows x64 安装版](https://github.com/xiaogouyo/snowdown/releases/download/v0.1.0/SnowDown_0.1.0_x64-setup.exe)
- [Windows x64 免安装版](https://github.com/xiaogouyo/snowdown/releases/download/v0.1.0/SnowDown_0.1.0_Windows_x64_portable.zip)
- [发布说明与全部附件](https://github.com/xiaogouyo/snowdown/releases/tag/v0.1.0)

需要 Windows 10/11 x64 和 WebView2 运行时。免安装版请完整解压后启动，保留 `resources` 文件夹。下载文件的 SHA256 校验值随 Release 提供。

## 首版功能

- BV号、av号、B站视频链接与 b23.tv 分享链接解析。
- 多P选择、当前账号可用画质选择、批量任务。
- MP4 视频与 M4A 原始音频，FFmpeg 自动合并。
- 下载队列、1至4任务并发、单任务限速、暂停/继续、失败重试。
- SQLite 保存设置与任务；退出后再次打开，可手动继续中断任务。
- 可选保存封面、XML弹幕、SRT字幕；附加资源不可用不影响已下载的视频。
- 系统托盘、单实例、打开下载目录与定位文件。
- 可选 SESSDATA 登录凭据。首版仅保存在当前进程内存，完全退出后清除；不读取浏览器凭据。

浏览器扩展、Linux和鸿蒙客户端尚未实现。界面组件和站点解析/任务模块已分离，方便后续适配。

## 使用

安装 `release/` 中的 Windows 安装程序，启动 SnowDown，粘贴视频链接并点击“解析视频”，选择分P与格式后下载。

也可解压免安装压缩包，双击文件夹中的 `SnowDown.exe`；请保留旁边的 `resources` 文件夹。

默认目录为系统 Downloads 下的 `SnowDown`。关闭主窗口会隐藏到系统托盘，下载继续；完全退出请使用托盘菜单。临时下载保存在所选目录的 `.snowdown/<任务ID>` 中，成功后清理该任务的临时文件。移除任务记录不会删除下载文件。

## 开发

需要 Node.js 22.12+、Rust stable MSVC、Visual Studio C++桌面工具与 WebView2。

```powershell
npm install
npm run setup:media
npm run desktop:dev
```

`npm run dev` 仅预览界面，解析与下载功能必须在桌面客户端运行。

```powershell
npm run build
npm test
cargo test --manifest-path src-tauri/Cargo.toml
npm run desktop:build
```

安装程序默认位于 `src-tauri/target/release/bundle/nsis/`。构建时将 FFmpeg 及许可证一并放入安装包；ffprobe 仅供开发验证使用。

## 实现边界

- 清晰度由B站返回的实际可用流决定。已登录用户可以填写自己的 SESSDATA 后重新解析；扫码登录尚未实现。
- 首版优先选择 AVC 视频与普通音频流；音频输出为 M4A，尚未提供MP3转码。
- 字幕和弹幕取决于源站可用性；XML为接口返回的弹幕快照，不保证完整历史弹幕。
- 页面暂不支持番剧、直播、收藏夹和UP主投稿列表链接。
- 暂停发生在媒体下载阶段；合并阶段可取消。应用重启后中断任务保持暂停，用户继续后重新解析资源地址并校验断点。
- 本机消息命令与下载核心分离；尚未注册浏览器 Native Messaging host。

## 依赖说明

参考 [THIRD_PARTY.md](THIRD_PARTY.md)。
