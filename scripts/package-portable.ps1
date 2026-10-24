$ErrorActionPreference = 'Stop'
$taskRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$taskVersion = (Get-Content -LiteralPath (Join-Path $taskRoot 'package.json') -Raw | ConvertFrom-Json).version
$taskRelease = Join-Path $taskRoot 'release'
$taskPortable = Join-Path $taskRelease 'SnowDown-Windows-x64'
$taskResolved = [IO.Path]::GetFullPath($taskPortable)
if (-not $taskResolved.StartsWith($taskRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Package path escaped workspace.' }
$taskResources = Join-Path $taskPortable 'resources'
New-Item -ItemType Directory -Force -Path $taskResources | Out-Null
Copy-Item -LiteralPath (Join-Path $taskRoot 'src-tauri\target\release\snowdown.exe') -Destination (Join-Path $taskPortable 'SnowDown.exe')
foreach ($taskFile in @('ffmpeg.exe', 'FFMPEG-LICENSE.txt', 'NOTICE.txt', 'ffmpeg-build.json')) {
    Copy-Item -LiteralPath (Join-Path $taskRoot ('src-tauri\resources\' + $taskFile)) -Destination (Join-Path $taskResources $taskFile)
}
Copy-Item -LiteralPath (Join-Path $taskRoot 'THIRD_PARTY.md') -Destination (Join-Path $taskPortable 'THIRD_PARTY.md')
@'
SnowDown Windows 0.1.0

使用 Windows 10/11 x64，系统需要 WebView2 运行时。
解压整个文件夹，双击 SnowDown.exe；请保留 resources 文件夹。

粘贴B站视频链接或BV号 → 解析视频 → 选择分P、画质和格式 → 下载。
默认保存到系统下载目录中的 SnowDown 文件夹，可在设置中修改。
关闭窗口后客户端在系统托盘继续运行，完全退出请使用托盘菜单。
应用重启后，中断任务保持暂停，点击继续可以恢复。

首版支持 MP4 视频、M4A 音频，以及可选的封面、XML弹幕和SRT字幕。
画质以当前账号实际可用资源为准。可在设置中填写自己的B站 SESSDATA；
此凭据只保留在当前进程内存，完全退出后清除。

移除下载记录不会删除已经保存的视频文件。
首版尚未提供扫码登录、浏览器扩展及其他站点支持。
'@ | Set-Content -LiteralPath (Join-Path $taskPortable '使用说明.txt') -Encoding utf8
$taskArchive = Join-Path $taskRelease ("SnowDown_$($taskVersion)_Windows_x64_portable.zip")
Compress-Archive -LiteralPath $taskPortable -DestinationPath $taskArchive -CompressionLevel Optimal -Force
$taskInstallerDir = Join-Path $taskRoot 'src-tauri\target\release\bundle\nsis'
if (Test-Path -LiteralPath $taskInstallerDir) {
    Get-ChildItem -LiteralPath $taskInstallerDir -Filter '*-setup.exe' | ForEach-Object { Copy-Item -LiteralPath $_.FullName -Destination (Join-Path $taskRelease $_.Name) }
}
Get-ChildItem -LiteralPath $taskRelease -File | Where-Object { $_.Extension -in @('.zip', '.exe') } | ForEach-Object {
    $taskHash = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLower()
    "$taskHash  $($_.Name)"
} | Set-Content -LiteralPath (Join-Path $taskRelease 'SHA256SUMS.txt') -Encoding ascii
Get-ChildItem -LiteralPath $taskRelease -File | Select-Object Name, Length
