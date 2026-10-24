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
抖音和小红书遇到访问限制时，点击链接下方“官方页面解析”，
完成登录或验证并播放当前视频，关闭官方窗口后自动返回解析。会话分别加密保存。
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
