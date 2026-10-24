# SnowDown

Windows 桌面视频下载工具，支持B站，并试验接入抖音、小红书视频。采用 Tauri 2、Vue 3、TypeScript 与 Rust，下载文件在本机处理和保存。

## 下载 v0.1.3

- [Windows x64 安装版](https://github.com/xiaogouyo/snowdown/releases/download/v0.1.3/SnowDown_0.1.3_x64-setup.exe)
- [Windows x64 免安装版](https://github.com/xiaogouyo/snowdown/releases/download/v0.1.3/SnowDown_0.1.3_Windows_x64_portable.zip)
- [发布说明与全部附件](https://github.com/xiaogouyo/snowdown/releases/tag/v0.1.3)

需要 Windows 10/11 x64 和 WebView2 运行时。免安装版请完整解压后启动，保留 `resources` 文件夹。下载文件的 SHA256 校验值随 Release 提供。

## 功能

- BV号、av号、B站视频链接与 b23.tv 分享链接解析。
- 抖音视频页、精选页 modal_id、分享短链接；小红书视频笔记与分享短链接。可粘贴完整分享文字，自动识别平台并保留必要的访问参数。
- 原生剪贴板识别：回到软件时提示，点击“粘贴并解析”或 Ctrl+Shift+V 快速解析。可在设置关闭识别，或开启识别后自动解析；不会自动创建下载任务。
- 多P选择、当前账号可用画质选择、批量任务。
- MP4 视频与 M4A 音频，FFmpeg 自动处理。
- 下载队列、1至4任务并发、单任务限速、暂停/继续、失败重试。
- SQLite 保存设置与任务；退出后再次打开，可手动继续中断任务。
- 可选保存封面、XML弹幕、SRT字幕；附加资源不可用不影响已下载的视频。
- 系统托盘、单实例、打开下载目录与定位文件。
- B站官方二维码登录，在手机B站确认；Windows DPAPI 加密保存会话，重启自动恢复，退出账号清除本机凭据。
- 抖音、小红书通过软件内的官方网页登录；分别加密保存网页会话。官方窗口仅用于登录，关闭后保存状态；输入链接并点击“解析视频”后在软件内选择下载。抖音支持分离音视频流合并。
- 状态、画质、并发数统一使用软件内的下拉菜单；长链接不会挤压操作按钮，设置操作区保持可见。
- 与顶部栏融为一体的窗口按钮，支持拖动、双击最大化与关闭到托盘。
- 二次元异形安装界面，透明立绘、曲面面板、安装路径选择与真实安装阶段提示。

浏览器扩展、Linux和鸿蒙客户端尚未实现。界面组件和站点解析/任务模块已分离，方便后续适配。

## 使用

安装 `release/` 中的 Windows 安装程序，启动 SnowDown，粘贴视频链接并点击“解析视频”，选择分P与格式后下载。

也可解压免安装压缩包，双击文件夹中的 `SnowDown.exe`；请保留旁边的 `resources` 文件夹。

默认目录为系统 Downloads 下的 `SnowDown`。关闭主窗口会隐藏到系统托盘，下载继续；完全退出请使用托盘菜单。临时下载保存在所选目录的 `.snowdown/<任务ID>` 中，成功后清理该任务的临时文件。移除任务记录不会删除下载文件。

登录：打开“设置 → 平台账号 → B站”，获取官方二维码，使用手机B站扫描并确认。凭据只由 Rust 后端接收，不返回网页界面；加密文件位于当前用户应用数据目录 `app.snowdown.desktop/bilibili-session.dat`，仅当前 Windows 用户可解密。启动时验证会话，临时断网会保留登录状态；B站会话失效后提示重新扫码。

抖音、小红书遇到访问限制时，点击链接下方的平台登录按钮，由你在官方窗口完成登录或验证。关闭窗口后保存登录状态，也可在“设置 → 平台账号”手动保存；输入框中的链接不会改变，点击“解析视频”才解析下载来源。软件会在后台复用自己的平台登录资料读取目标视频，正常解析不需要手动打开网页播放。应用只读取自己官方窗口的会话，独立网页资料位于 `official-profiles/<平台>`，加密导入会话位于 `site-sessions/<平台>.dat`；不会读取外部浏览器账号。清除导入会话后，官方窗口中的账号仍需在平台页面自行退出。会话有有效期，限制访问的内容不能保证可下载。

剪贴板仅在主窗口聚焦时检查，后端只返回识别到的平台链接；不会保存其他剪贴板内容。必要的分享访问参数保留在本机任务来源中，界面摘要不显示这些参数。

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

异形安装程序位于 `src-tauri/target/release/bundle/snowdown/`，内部 NSIS 负载位于 `bundle/nsis/`。构建时将 FFmpeg 及许可证一并放入安装包；ffprobe 仅供开发验证使用。安装器使用 Windows .NET Framework 4.6.2+，通常已随 Windows 10/11 安装，不需要额外安装 .NET Desktop Runtime。安装器构建和隔离测试说明见 [installer/README.md](installer/README.md)。

## 实现边界

- 清晰度由B站返回的实际可用流决定。扫码登录后重新解析，可更新当前账号可用画质。登录会话受B站有效期和风控限制，客户端不会绕过权限或承诺永久有效。
- 首版优先选择 AVC 视频与普通音频流；音频输出为 M4A，尚未提供MP3转码。
- 字幕和弹幕取决于源站可用性；XML为接口返回的弹幕快照，不保证完整历史弹幕。
- 页面暂不支持番剧、直播、收藏夹和UP主投稿列表链接。
- 抖音、小红书首批仅支持视频，不支持图集、图文、合集和私密内容。平台可能要求登录、验证或有效的分享参数。B站和抖音样例已实际下载并验证音视频轨道，小红书由用户反馈正常；具体范围见 Release 附件。网站数据结构变化可能影响解析。
- 暂停发生在媒体下载阶段；合并阶段可取消。应用重启后中断任务保持暂停，用户继续后重新解析资源地址并校验断点。
- 本机消息命令与下载核心分离；尚未注册浏览器 Native Messaging host。

## 依赖说明

参考 [THIRD_PARTY.md](THIRD_PARTY.md)。
