# SnowDown 隐私政策 / Privacy Policy

生效日期 / Effective date：2026-10-30  
维护者 / Maintainer：[xiaogouyo](https://github.com/xiaogouyo)  
项目 / Project：[SnowDown](https://github.com/xiaogouyo/SnowDown)

本政策适用于 SnowDown 浏览器扩展，包括 Chrome 商店上传版 v0.1.17，以及扩展与本机 SnowDown Windows 桌面端之间的账号同步和下载联动。界面语言不同不改变数据处理方式。后续版本如改变处理方式，将更新本政策。

## 1. 用途与处理原则

SnowDown 帮助用户识别、筛选、预览和保存网页中的视频、音频、图片及文档，提供批量下载和 ZIP 打包，并可将下载或解析任务转交本机桌面端。扩展没有用于汇集用户资源、Cookie 或浏览记录的开发者云端服务，不使用广告或分析 SDK，不出售用户数据，不将数据用于广告定向、信用评估或与上述功能无关的用途。

我们对信息的使用和转移遵守 Chrome Web Store 用户数据政策，包括 Limited Use（有限使用）要求。处理的信息仅用于提供或改进用户可见的资源识别、下载和本机联动功能。

## 2. 扩展会处理哪些信息

- **网页及媒体资源信息**：已获授权标签页的 URL、标题、媒体元素和元数据、封面、图片尺寸、媒体请求地址、内容类型、文件大小以及部分平台接口中与媒体识别相关的数据。扩展可在授权站点观察媒体请求并显示封面下载入口；不只有点击“读取”时才发生识别。
- **平台登录信息**：在桌面连接且账号同步开关开启时，读取 B站、抖音、小红书对应域名的 Cookie，其中可能包含身份验证会话及其他站点 Cookie。它们用于使本机桌面端使用用户已拥有的平台访问权限。
- **账号资料**：从支持的平台页面获取昵称和头像等资料，用于本机账号状态显示；并非每个平台每次都能取得这些资料。
- **设置与连接信息**：下载方式、账号同步和视频下载入口开关、桌面连接及绑定标识、插件与桌面版本、上次同步时间及平台。
- **下载和打包任务**：所选资源 URL、文件名、格式或画质、ZIP 名称、任务状态、下载文件及打包临时文件。

扩展不使用浏览器完整历史记录 API，不主动读取密码输入框、支付信息、联系人或个人通信，也不提供与下载无关的按键或点击行为分析。用户主动选择的文档、URL 或网页资源仍可能包含个人信息，请按需要选择资源。

## 3. 账号同步的默认行为

**v0.1.17 的账号同步开关默认开启。发现新的桌面绑定时，当前实现也会开启桌面转发与账号同步。桌面端连接且同步开启时，可能自动同步支持平台的登录状态，并在 Cookie 变化后再次同步。**

用户可在插件“偏好设置”中关闭“连接后同步三平台账号”。关闭后停止后续自动同步；这不会自动删除此前已经保存在桌面端的登录信息。要移除桌面端已保存的会话，请同时在桌面端的平台账号设置中退出对应账号。卸载扩展也不等于删除桌面端会话。

## 4. 信息在哪里处理和传输

解析、缓存、资源筛选和打包主要在用户设备上完成。启用本机联动时，扩展通过 Chrome Native Messaging 与已安装的 SnowDown 本机桥接程序通信；桌面端收到的任务信息可包括源页面 URL、媒体详情、所选画质、资源地址、文件名，以及同步开启时的平台 Cookie、昵称和头像。它们不会为账号同步而发送给开发者云端。

解析、预览和下载需要向对应网站、平台接口、资源服务器或 CDN 发起网络请求。这些服务会收到正常网络请求信息，如 IP 地址、所请求 URL、必要的请求头，以及浏览器或桌面端按该站点会话发送的凭据。桌面端处理转发任务时也会向资源网站发起请求。用户主动打开 GitHub 下载、项目或问题页面时，GitHub 会接收该次访问。相关第三方依其各自政策处理数据，SnowDown 无法控制其服务器日志。

资源列表可包含 HTTP 地址；访问这类资源不具有 HTTPS 的传输加密保障。平台账号与 Cookie 同步使用本机通信通道，不通过公开 HTTP 网站传递。

## 5. 本机保存与保留时间

- 设置、绑定状态和上次同步信息保存在浏览器扩展本机存储中，直到被覆盖、重置或随扩展数据清除。扩展不使用 Chrome 同步存储将这些设置上传到 Chrome 账号。
- 解析缓存及观察到的资源列表主要位于扩展进程内存中；解析缓存会短期过期，页面切换、标签页关闭或进程结束也会使相关状态失效。
- 下载和打包任务的部分元数据使用浏览器会话存储。浏览器下载列表可能保留下载记录，已保存文件留在用户选择的下载目录。
- ZIP 打包会使用浏览器的本机临时文件存储。完成、取消或失败时尝试清理临时文件；异常关闭可能留下临时文件，后续打包会对本扩展较旧的临时文件进行有界清理。
- 桌面端会保存用户选择的任务记录、设置和登录状态；平台 Cookie 使用 Windows 数据保护机制（DPAPI）加密保存。桌面端账号资料或缓存的保护方式依其具体存储位置而定，不能将所有本机文件视为已加密。

关闭浏览器或退出软件不会自动删除下载内容或所有桌面端数据。

## 6. 用户的选择与删除

用户可通过浏览器扩展管理撤回网站权限、停用或卸载扩展；在插件设置中关闭桌面转发、账号同步或页面下载入口；在浏览器中清除扩展数据及下载记录；在桌面端退出平台账号并管理任务记录。撤回可选网站权限会限制相应站点的识别能力。要退出平台本身的登录或撤销会话，请使用平台提供的账号安全功能。

下载文件由用户自行管理。删除下载记录或卸载扩展通常不会删除已保存文件；桌面端保存的登录状态和设置也应单独管理。

## 7. 权限与执行代码

扩展使用当前标签页访问、脚本执行、网站访问、网络请求观察、本机存储、下载、Cookie、原生消息、定时任务及离屏解析权限，以实现上述功能。特定请求头规则仅针对扩展自身发起的指定平台媒体请求。其他网站的广泛 HTTP/HTTPS 访问为可选权限，由浏览器授权机制控制。

扩展自身的 JavaScript、Worker 和第三方媒体处理依赖包含在安装包中，不从开发者服务器下载代码执行。读取网页、接口数据和媒体资源不意味着这些资源由 SnowDown 托管。

## 8. 第三方共享与人工访问

开发者不通过扩展后台接收上述用户数据，不出售或出租这些数据，也不向广告商、数据经纪商或分析服务提供数据。必要的资源请求由用户设备直接发往目标网站；本机联动传送给用户自己的桌面端。

若用户主动通过 GitHub Issues 提供截图、日志或链接，这些内容可能公开，并由 GitHub 存储。请勿提交 Cookie、令牌、密码、带敏感签名的完整链接或其他秘密信息。此类主动提交不属于扩展自动上传。

## 9. 政策变更与联系

本政策更新时将修改生效日期，并保留在本仓库的公开提交历史中。涉及数据处理方式的重要变化，应与相应版本说明及产品内说明保持一致。

隐私问题或数据处理疑问可通过 [SnowDown GitHub Issues](https://github.com/xiaogouyo/SnowDown/issues) 联系维护者；请只提供不含秘密信息的描述。开发者没有可以代用户删除的云端资源或账号会话数据库；本机数据应按上述方式在用户设备上管理。

---

## English summary

SnowDown processes permitted website URLs, titles, media/resource metadata, selected downloads, preferences, and local connection/task information to identify and save website resources. While desktop account synchronization is enabled and the local desktop application is connected, it reads cookies for Bilibili, Douyin, and Xiaohongshu and may read account nicknames and avatars. These are transferred to the user's own desktop application using Native Messaging. In v0.1.17, synchronization is enabled by default, and a new desktop binding enables desktop forwarding and account synchronization. Disable synchronization in extension settings to stop future synchronization; sign out separately in desktop account settings to remove previously saved desktop sessions.

The extension does not operate a developer cloud collection service, use advertising/analytics SDKs, sell personal data, or use data for unrelated advertising or credit assessments. Website/CDN requests necessarily disclose ordinary network/request information to the target service. Some user-selected resources can use HTTP. Settings are stored locally, parse caches are primarily in memory, download jobs use session storage and temporary local files, and saved downloads remain on the device. The Windows companion encrypts saved platform cookies using DPAPI. Uninstalling the extension does not remove desktop sessions or downloaded files.

Users can revoke site permissions, disable features, clear extension data and download records, sign out of desktop platform accounts, or uninstall the extension. The extension's executable code is packaged locally. Our use and transfer of information follow the Chrome Web Store User Data Policy, including its Limited Use requirements. For questions, contact the maintainer through the public project Issues page without submitting cookies, tokens, passwords, or other sensitive details. The Chinese sections above provide the full policy.