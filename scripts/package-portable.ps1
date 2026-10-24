$ErrorActionPreference = 'Stop'
$taskRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$taskVersion = (Get-Content -LiteralPath (Join-Path $taskRoot 'package.json') -Raw | ConvertFrom-Json).version
$taskRelease = Join-Path $taskRoot 'release'
$taskPortable = Join-Path $taskRelease "SnowDown-$taskVersion-Windows-x64"
$taskResolved = [IO.Path]::GetFullPath($taskPortable)
if (-not $taskResolved.StartsWith($taskRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Package path escaped workspace.' }
$taskResources = Join-Path $taskPortable 'resources'
New-Item -ItemType Directory -Force -Path $taskResources | Out-Null
Copy-Item -LiteralPath (Join-Path $taskRoot 'src-tauri\target\release\snowdown.exe') -Destination (Join-Path $taskPortable 'SnowDown.exe')
foreach ($taskFile in @('ffmpeg.exe', 'FFMPEG-LICENSE.txt', 'NOTICE.txt', 'ffmpeg-build.json')) {
    Copy-Item -LiteralPath (Join-Path $taskRoot ('src-tauri\resources\' + $taskFile)) -Destination (Join-Path $taskResources $taskFile)
}
Copy-Item -LiteralPath (Join-Path $taskRoot 'THIRD_PARTY.md') -Destination (Join-Path $taskPortable 'THIRD_PARTY.md')
@"
SnowDown Windows $taskVersion

使用 Windows 10/11 x64，系统需要 WebView2 运行时。
解压整个文件夹，双击 SnowDown.exe；请保留 resources 文件夹。

粘贴B站、抖音或小红书视频链接 → 解析视频 → 选择画质和格式 → 下载。
点击“批量下载”或 Ctrl+N，可混合粘贴多个平台链接，用换行、空格或逗号分隔。
统一设置格式、画质、保存目录、附件、分P范围和任务启动间隔后，一键加入下载队列。
每批最多100个链接、1000个分P任务；自动去重，解析失败的链接保留重试。
同批启动间隔为0至3600秒，从上一个任务实际启动时计算，仍遵守全局并发数。
批量设置仅用于本批任务，不修改全局下载设置；0秒表示不额外等待。
点击右上角版本号，或在“关于”中点击“检查更新”；启动后及每6小时也会自动检查GitHub正式版本。
“暂不更新”会在24小时内停止自动提醒同一版本，手动检查仍可用。
更新包可在软件内下载、取消和重试，校验通过后可退出并打开安装器。
安装前请先暂停或完成下载任务。免安装版若保持免安装方式，请在GitHub下载ZIP，退出后解压覆盖。
支持完整分享文字；点击“粘贴并解析”或按 Ctrl+Shift+V 快速解析。
回到软件自动识别剪贴板，可在设置中关闭，或开启识别后自动解析。
默认保存到系统下载目录中的 SnowDown 文件夹，可在设置中修改。
关闭窗口后客户端在系统托盘继续运行，完全退出请使用托盘菜单。
应用重启后，中断任务保持暂停，点击继续可以恢复。

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
首批支持视频笔记，不支持图集、直播或私密内容。当前版本尚未提供浏览器扩展。
"@ | Set-Content -LiteralPath (Join-Path $taskPortable '使用说明.txt') -Encoding utf8
$taskArchive = Join-Path $taskRelease ("SnowDown_$($taskVersion)_Windows_x64_portable.zip")
Compress-Archive -LiteralPath $taskPortable -DestinationPath $taskArchive -CompressionLevel Optimal -Force
$taskInstallerDir = Join-Path $taskRoot 'src-tauri\target\release\bundle\snowdown'
if (Test-Path -LiteralPath $taskInstallerDir) {
    Get-ChildItem -LiteralPath $taskInstallerDir -Filter "SnowDown_$($taskVersion)_*-setup.exe" | ForEach-Object { Copy-Item -LiteralPath $_.FullName -Destination (Join-Path $taskRelease $_.Name) }
}
Get-ChildItem -LiteralPath $taskRelease -File | Where-Object { $_.Name -like "SnowDown_$($taskVersion)_*" -and $_.Extension -in @('.zip', '.exe') } | ForEach-Object {
    $taskHash = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLower()
    "$taskHash  $($_.Name)"
} | Set-Content -LiteralPath (Join-Path $taskRelease 'SHA256SUMS.txt') -Encoding ascii
Get-ChildItem -LiteralPath $taskRelease -File | Select-Object Name, Length
