# SnowDown 浏览器插件 v0.1.0

支持 Google Chrome / Microsoft Edge（Chromium 116 或更新版本）。这是 Manifest V3 插件，可独立下载，也可连接 Windows 桌面版 SnowDown v0.2.0。

## 安装

推荐从桌面软件左侧“浏览器插件”贴片选择浏览器。软件会准备稳定的插件目录、注册当前用户的原生消息通道、打开扩展管理页与文件夹。开启开发者模式，选择“加载已解压的扩展”，选择软件显示的目录即可。首次连接默认启用桌面转发和账号同步，可在插件设置中关闭。

独立使用：下载 `SnowDown_BrowserExtension_0.1.0.zip` 并完整解压。在 `chrome://extensions` 或 `edge://extensions` 开启开发者模式，加载包含 `manifest.json` 的文件夹。固定工具栏 SnowDown 图标。

普通 Windows 用户不能由软件静默安装未上架的扩展。当前未上架扩展商店，最后的浏览器加载确认需用户完成；不设置企业强制安装策略，不修改用户浏览器的 Preferences 或 Secure Preferences。官方说明：[Chrome](https://developer.chrome.com/docs/extensions/how-to/distribute/install-extensions)、[Edge](https://learn.microsoft.com/en-us/deployedge/microsoft-edge-manage-extensions-webstore)。

## 使用

打开B站、抖音或小红书的目标视频，点击插件图标，再点“读取当前视频”。浏览器中已有的账号会话直接用于解析。抖音和小红书需先播放视频，安装后的已有标签页可刷新一次，使插件观察到视频详情。选择画质后下载。

点击弹出界面左上角 SnowDown 标志或“网页资源”切换模式，首次需批准网页站点访问权限。可筛选视频、图像、音频、格式、最小宽高和地址关键词，再批量下载所选资源。滚动页面或播放媒体后刷新列表，可发现新加载资源。

未连接桌面端时使用浏览器下载。分离的 MP4 视频和音频在插件自己的下载页合并，下载期间保留该页；大文件建议使用桌面端。直接媒体交给浏览器下载列表。桌面端运行后显示“桌面已连接”，可在设置中切换桌面转发。软件离线时自动停用有效转发并回退浏览器，恢复后沿用用户偏好。需要软件时可从设置打开官方 GitHub 下载页。

普通 HTTPS 资源可交给桌面队列，使用断点与分段下载；本机地址、不支持的文件格式或跳转型资源可改用浏览器。网页资源列表仅包含已加载或已请求并可观察到的资源，不保证列出尚未播放、跨站受限、DRM 或私密媒体。播放清单和 blob 分段流不能作为普通 MP4 直接保存。当前不提供 DRM 解密、直播录制或小红书图集的平台解析。

## 账号与隐私

账号同步是可关闭的选项。仅读取B站、抖音和小红书的 Cookie，通过允许固定插件 ID 的原生消息通道同步到本机，桌面端使用 Windows DPAPI 加密保存。Cookie 不写入插件的 storage 或日志，不上传到 SnowDown 服务器；本地服务没有网页可访问的 HTTP 接口。其他站点的 Cookie 不会导入桌面端。

浏览器中未登录的平台不会清除桌面端已保存账号。插件获取的头像只来自对应账号控件；官方页面未提供头像时显示默认图标。资源和视频签名地址仅用于本机下载任务。网页资源读取权限与平台权限分开，可在浏览器中撤回。桌面软件关闭后插件仍可独立运行。

## 开发

在仓库根目录执行 `npm ci` 与 `node scripts/build-extension.mjs`。构建目录为 `src-tauri/resources/browser-extension`，不提交生成文件。`identity.json` 仅包含公钥和稳定插件 ID，不包含私钥；不要随意修改，以免破坏原生通道允许列表。浏览器合并使用 Mediabunny，完整许可证随包提供；源代码与锁定版本来自本仓库及其官方项目。
