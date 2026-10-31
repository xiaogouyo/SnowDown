$ErrorActionPreference = 'Stop'
$taskRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$taskVersion = (Get-Content -LiteralPath (Join-Path $taskRoot 'package.json') -Raw | ConvertFrom-Json).version
$taskExtensionVersion = (Get-Content -LiteralPath (Join-Path $taskRoot 'src-tauri/resources/browser-extension/manifest.json') -Raw -Encoding UTF8 | ConvertFrom-Json).version
$taskRelease = Join-Path $taskRoot 'release'
$taskPortable = Join-Path $taskRelease "SnowDown-$taskVersion-Windows-x64"
$taskResolved = [IO.Path]::GetFullPath($taskPortable)
if (-not $taskResolved.StartsWith($taskRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Package path escaped workspace.' }
$taskResources = Join-Path $taskPortable 'resources'
New-Item -ItemType Directory -Force -Path $taskResources | Out-Null
Copy-Item -LiteralPath (Join-Path $taskRoot 'src-tauri\target\release\snowdown.exe') -Destination (Join-Path $taskPortable 'SnowDown.exe')
foreach ($taskFile in @('ffmpeg.exe', 'ffprobe.exe', 'yt-dlp.exe', 'deno.exe', 'EXTRACTOR-LICENSE.txt', 'extractor-build.json', 'FFMPEG-LICENSE.txt', 'NOTICE.txt', 'ffmpeg-build.json')) {
    Copy-Item -LiteralPath (Join-Path $taskRoot ('src-tauri\resources\' + $taskFile)) -Destination (Join-Path $taskResources $taskFile)
}
Copy-Item -LiteralPath (Join-Path $taskRoot 'src-tauri\resources\browser-extension') -Destination $taskResources -Recurse -Force
Copy-Item -LiteralPath (Join-Path $taskRoot 'THIRD_PARTY.md') -Destination (Join-Path $taskPortable 'THIRD_PARTY.md')
Copy-Item -LiteralPath (Join-Path $taskRoot 'PRIVACY.md') -Destination (Join-Path $taskPortable 'PRIVACY.md')
@"
SnowDown Windows $taskVersion

使用 Windows 10/11 x64，系统需要 WebView2 运行时。
解压整个文件夹，双击 SnowDown.exe；请保留 resources 文件夹。

粘贴B站、抖音、小红书、视频号或共享目录中的其他平台链接 → 解析视频 → 选择画质和格式 → 下载。
新增站点以公开页面或浏览器当前已加载媒体为基础，部分内容需登录或可能受平台限制。
视频号：设置 → 平台账号 → 视频号 → 打开腾讯元宝官方登录；完成后返回保存会话。再粘贴视频号分享链接解析下载。
视频号解析会话单独管理，仅在本机加密保存，不读取外部浏览器或微信账号数据库；可在对应账号页检查或退出。
视频号首批支持官方接口返回的普通可播放视频；画质仅展示实际取得的选项。源站不支持分段时自动使用单连接，重新继续任务时可能需要从头传输。
插件偏好设置中开启“启用更多平台与预览下载”，确认权限并刷新网页，可使用预览卡片下载菜单。
点击“批量下载”或 Ctrl+N，可混合粘贴多个平台链接，用换行、空格或逗号分隔。
统一设置格式、画质、保存目录、附件、分P范围和任务启动间隔后，一键加入下载队列。
点击左上角 SnowDown 标志切换简约模式，再次点击返回；模式切换有动画并记住偏好。
简约模式初始只显示居中输入框，识别链接后自动解析，也可按 Enter 立即解析。
解析时输入框缩小上移，下方直接展开视频信息、下载设置和进度，不弹出下载选择窗口。
可在页内设置格式、画质、目录、附件和分P，单次设置不改变全局偏好。
输入框清除按钮或 Ctrl+N 可重新开始，正在下载的任务继续运行。
每批最多100个链接、1000个分P任务；自动去重，解析失败的链接保留重试。
同批启动间隔为0至3600秒，从上一个任务实际启动时计算，仍遵守全局并发数。
批量设置仅用于本批任务，不修改全局下载设置；0秒表示不额外等待。
点击右上角版本号，或在“关于”中点击“检查更新”；启动后及每6小时也会自动检查GitHub正式版本。
“暂不更新”会在24小时内停止自动提醒同一版本，手动检查仍可用。
更新包下载前会实时检测66个镜像，按延迟优先选择；失败自动切换并最终尝试GitHub官方直连。
可在更新弹窗展开镜像库查看节点状态、实测延迟与当前下载源，测速和下载均可暂停。
“后台下载”仅收起更新面板，原生任务继续；右上角版本入口可查看实际进度和待安装状态。
切换到简约模式或关闭主窗口到托盘，也会继续下载；可返回完整模式查看更新。
更新包可在软件内暂停、继续和重试，重开软件后保留进度，核对官方GitHub的SHA256后会弹出右下角通知，点击安装更新即可自动覆盖并重启。
安装时会先保存下载进度并暂停任务；账号登录、设置、任务和下载内容保留。免安装版若保持免安装方式，请在GitHub下载ZIP，退出后解压覆盖。
支持完整分享文字；点击“粘贴并解析”或按 Ctrl+Shift+V 快速解析。
原生后台持续识别剪贴板链接，关闭到托盘后仍生效，可在设置中关闭。
开启自动解析后在后台准备最新结果，回到窗口展示，不自动创建下载任务。
手动编辑的输入不会被替换；其他弹窗打开时延后展示，可通过“查看结果”使用已解析结果。
剪贴板临时占用会重试，重新复制失败的链接可重试解析；不会保存剪贴板历史。
默认保存到系统下载目录中的 SnowDown 文件夹，可在设置中修改。
关闭窗口后客户端在系统托盘继续运行，右键打开统一风格的圆角菜单。
菜单支持方向键、Esc 和失焦关闭；完全退出请使用托盘菜单。
应用重启后，中断任务保持暂停，点击继续可以恢复。
视频与更新默认每文件8连接，可在设置中选择1/2/4/8/16连接；所有分段共同遵守单任务限速。
分段进度写盘保存，暂停、断网或重开后可续传；同一更新包可跨镜像继续，最终校验官方摘要。
服务器不支持安全分段或资源发生变化时会重新下载。实际提速取决于网络和站点限制。

支持 MP4 视频、M4A 音频，以及可选的封面、XML弹幕和SRT字幕。
在设置 → 平台账号 → B站中获取官方二维码，用手机B站扫码并确认。
登录状态由 Windows 为当前系统用户加密保存，重启后自动恢复。
B站会话失效后需重新扫码；退出账号会删除本机保存的登录状态。
画质以当前账号实际可用资源为准。

移除下载记录不会删除已经保存的视频文件。
抖音、小红书也可在平台账号中获取官方登录二维码，使用对应手机 App 扫码确认。
三个平台二维码均会在过期后自动更新，关闭账号面板或切换平台后停止轮询。
登录成功后加密保存；已登录时可“退出账号”或“检查登录”。
退出会清除软件内该平台的会话和网页登录资料，不影响外部浏览器账号。
登录只负责账号授权，粘贴目标视频链接并点击“解析视频”才开始解析。
首批支持视频笔记，不支持图集、直播或私密内容。浏览器插件 v$taskExtensionVersion 可独立使用或连接桌面端。
从左侧浏览器插件贴片选择 Chrome，打开官方商店安装；浏览器负责商店版自动更新。Edge 可继续使用本地安装。ZIP 为商店无法访问时的备用方式。
桌面同时兼容商店正式 ID 和原 ZIP 插件 ID；首次连接自动绑定，可在插件设置中调整下载和账号同步。
联动安装不可用时，可在插件面板“下载 ZIP”，将内置插件离线保存到本机。
解压 ZIP 后，在浏览器扩展管理页开启开发者模式，加载包含 manifest.json 的文件夹，并固定插件图标。
普通 Windows 浏览器不能静默安装未上架扩展，不会修改企业策略。
点击插件 SnowDown 标志切换平台视频与网页资源筛选；可选浏览器下载或转发到软件。
软件离线时自动回退浏览器；账号同步可在插件设置关闭，仅在本机传输并加密保存。
"@ | Set-Content -LiteralPath (Join-Path $taskPortable '使用说明.txt') -Encoding utf8
$taskArchive = Join-Path $taskRelease ("SnowDown_$($taskVersion)_Windows_x64_portable.zip")
Compress-Archive -LiteralPath $taskPortable -DestinationPath $taskArchive -CompressionLevel Optimal -Force
$taskInstallerDir = Join-Path $taskRoot 'src-tauri\target\release\bundle\snowdown'
if (Test-Path -LiteralPath $taskInstallerDir) {
    Get-ChildItem -LiteralPath $taskInstallerDir -Filter "SnowDown_$($taskVersion)_*-setup.exe" | ForEach-Object { Copy-Item -LiteralPath $_.FullName -Destination (Join-Path $taskRelease $_.Name) }
}
$taskExtensionName="SnowDown_BrowserExtension_$taskExtensionVersion.zip"
$taskExtensionPackage=Join-Path $taskRelease $taskExtensionName
if (Test-Path -LiteralPath $taskExtensionPackage) {
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $taskExtensionSource=Join-Path $taskRoot 'src-tauri/resources/browser-extension'
    $taskPublicFiles=@(Get-ChildItem -LiteralPath $taskExtensionSource -Recurse -File | ForEach-Object { $_.FullName.Substring($taskExtensionSource.Length+1).Replace('\','/') })
    $taskExistingZip=[IO.Compression.ZipFile]::OpenRead($taskExtensionPackage)
    try {
        $taskEntries=@($taskExistingZip.Entries | Where-Object { $_.Name })
        if (@(Compare-Object $taskPublicFiles @($taskEntries | ForEach-Object { $_.FullName.Replace('\','/') })).Count) { throw 'Existing extension ZIP differs; publish a new extension version instead of replacing it.' }
        foreach($taskEntry in $taskEntries) {
            $taskRelative=$taskEntry.FullName.Replace('\','/')
            $taskStream=$taskEntry.Open();$taskHasher=[Security.Cryptography.SHA256]::Create()
            try { $taskEntryHash=[BitConverter]::ToString($taskHasher.ComputeHash($taskStream)).Replace('-','') } finally { $taskStream.Dispose();$taskHasher.Dispose() }
            if($taskEntryHash -ne (Get-FileHash -LiteralPath (Join-Path $taskExtensionSource $taskRelative) -Algorithm SHA256).Hash) { throw 'Existing extension ZIP contents differ; increment its version before packaging.' }
        }
    } finally { $taskExistingZip.Dispose() }
} else {
    Compress-Archive -Path (Join-Path $taskRoot 'src-tauri\resources\browser-extension\*') -DestinationPath $taskExtensionPackage -CompressionLevel Optimal
}
Get-ChildItem -LiteralPath $taskRelease -File | Where-Object { ($_.Name -like "SnowDown_$($taskVersion)_*" -or $_.Name -eq $taskExtensionName) -and $_.Extension -in @('.zip', '.exe') } | ForEach-Object {
    $taskHash = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLower()
    "$taskHash  $($_.Name)"
} | Set-Content -LiteralPath (Join-Path $taskRelease 'SHA256SUMS.txt') -Encoding ascii
Get-ChildItem -LiteralPath $taskRelease -File | Select-Object Name, Length
