$ErrorActionPreference = 'Stop'
$taskRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$taskVersion = (Get-Content -LiteralPath (Join-Path $taskRoot 'package.json') -Raw | ConvertFrom-Json).version
$taskId = [Guid]::NewGuid().ToString('N')
$taskQaRoot = [IO.Path]::GetFullPath((Join-Path $taskRoot ".tools\installer-qa-$taskId"))
if (-not $taskQaRoot.StartsWith($taskRoot + [IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)) { throw 'QA path escaped workspace.' }
New-Item -ItemType Directory -Force -Path $taskQaRoot | Out-Null
$taskInstall = Join-Path $taskQaRoot '应用 空格\SnowDown'
$taskProduct = "SnowDown-QA-$taskId"
$taskRegistry = "Software\Microsoft\Windows\CurrentVersion\Uninstall\$taskProduct"
$taskMakerRegistry = "Software\SnowDown-QA\$taskProduct"
$taskGenerated = Join-Path $taskRoot 'src-tauri\target\release\nsis\x64'
$taskScript = Get-Content -LiteralPath (Join-Path $taskGenerated 'installer.nsi') -Raw -Encoding utf8
$taskPayload = Join-Path $taskQaRoot 'payload.exe'
$taskScript = $taskScript.Replace('!define PRODUCTNAME "SnowDown"',"!define PRODUCTNAME `"$taskProduct`"")
$taskScript = $taskScript.Replace('!define MANUFACTURER "snowdown"','!define MANUFACTURER "SnowDown-QA"')
$taskScript = $taskScript.Replace('!define BUNDLEID "app.snowdown.desktop"',"!define BUNDLEID `"app.snowdown.qa.$taskId`"")
$taskScript = $taskScript.Replace('!define OUTFILE "nsis-output.exe"',"!define OUTFILE `"$taskPayload`"")
$taskScriptPath = Join-Path $taskQaRoot 'qa.nsi'
$taskScript | Set-Content -LiteralPath $taskScriptPath -Encoding utf8
$taskCompiler = Join-Path $taskRoot 'src-tauri\target\.tauri\NSIS\makensis.exe'
$taskUninstall = Join-Path $taskInstall 'uninstall.exe'
$taskUserRegistry = [Microsoft.Win32.RegistryKey]::OpenBaseKey([Microsoft.Win32.RegistryHive]::CurrentUser,[Microsoft.Win32.RegistryView]::Registry64)
try {
    & $taskCompiler /V2 "/X!addincludedir `"$taskGenerated`"" $taskScriptPath
    if ($LASTEXITCODE -ne 0) { throw 'QA NSIS compilation failed.' }
    $taskWrapper = Join-Path $taskQaRoot 'SnowDown-QA-setup.exe'
    & (Join-Path $taskRoot 'scripts\build-installer.ps1') -SkipTests -PayloadPath $taskPayload -OutputPath $taskWrapper
    # Start with an actual archived pre-auto-update binary, an older registry
    # version and no ownership manifest. Keep product/registry fully isolated.
    $taskLegacyExe = Join-Path $taskRoot 'release\SnowDown-0.1.10-Windows-x64\SnowDown.exe'
    if (-not (Test-Path -LiteralPath $taskLegacyExe)) { throw 'Archived v0.1.10 binary required for cross-version QA.' }
    $taskMainDefinition = [regex]::Match($taskScript,'(?m)^!define MAINBINARYSRCPATH "([^\r\n]+)"')
    $taskLegacyPayload = Join-Path $taskQaRoot 'legacy-payload.exe'
    $taskLegacyScript = $taskScript.Replace($taskMainDefinition.Groups[1].Value,$taskLegacyExe).Replace($taskPayload,$taskLegacyPayload)
    $taskLegacyScript = $taskLegacyScript.Replace("!define VERSION `"$taskVersion`"",'!define VERSION "0.1.10"').Replace("!define VERSIONWITHBUILD `"$taskVersion.0`"",'!define VERSIONWITHBUILD "0.1.10.0"')
    $taskLegacyNsi = Join-Path $taskQaRoot 'legacy.nsi'
    $taskLegacyScript | Set-Content -LiteralPath $taskLegacyNsi -Encoding utf8
    & $taskCompiler /V2 "/X!addincludedir `"$taskGenerated`"" $taskLegacyNsi
    if ($LASTEXITCODE -ne 0) { throw 'Legacy fixture compilation failed.' }
    $taskLegacy = Start-Process -FilePath $taskLegacyPayload -ArgumentList "/S /NS /D=$taskInstall" -PassThru -Wait -WindowStyle Hidden
    if ($taskLegacy.ExitCode -ne 0) { throw 'Legacy fixture installation failed.' }
    $taskLegacyHash = (Get-FileHash -LiteralPath (Join-Path $taskInstall 'snowdown.exe')).Hash
    if ($taskLegacyHash -ne (Get-FileHash -LiteralPath $taskLegacyExe).Hash) { throw 'Legacy binary was not installed.' }
    foreach ($taskProtected in @('Cookies','settings.json','tasks.db','download.mp4')) { [IO.File]::WriteAllText((Join-Path $taskInstall $taskProtected),'legacy-'+$taskProtected) }
    $taskResult = Start-Process -FilePath $taskWrapper -ArgumentList @('/S',"`"/D=$taskInstall`"") -PassThru -Wait -WindowStyle Hidden
    if ($taskResult.ExitCode -ne 0) { throw "Shaped installer silent flow failed: $($taskResult.ExitCode)" }
    foreach ($taskPair in @(@('src-tauri\target\release\snowdown.exe','snowdown.exe'),@('src-tauri\resources\ffmpeg.exe','resources\ffmpeg.exe'))) {
        $taskSourceHash = (Get-FileHash -LiteralPath (Join-Path $taskRoot $taskPair[0])).Hash
        $taskTargetHash = (Get-FileHash -LiteralPath (Join-Path $taskInstall $taskPair[1])).Hash
        if ($taskSourceHash -ne $taskTargetHash) { throw 'Installed file integrity failed.' }
    }
    $taskKey = $taskUserRegistry.OpenSubKey($taskRegistry)
    if (-not $taskKey -or $taskKey.GetValue('DisplayVersion') -ne $taskVersion) { throw 'QA add/remove programs entry missing.' }
    $taskKey.Dispose()
    foreach ($taskProtected in @('Cookies','settings.json','tasks.db','download.mp4')) { if ([IO.File]::ReadAllText((Join-Path $taskInstall $taskProtected)) -ne 'legacy-'+$taskProtected) { throw 'Cross-version update changed user data.' } }
    # A locked component must reject the upgrade before replacing the executable.
    $taskBeforeLock = (Get-FileHash -LiteralPath (Join-Path $taskInstall 'snowdown.exe')).Hash
    $taskLock = [IO.File]::Open((Join-Path $taskInstall 'resources\ffmpeg.exe'),[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::None)
    try {
        $taskLocked = Start-Process -FilePath $taskWrapper -ArgumentList @('/S',"`"/D=$taskInstall`"") -PassThru -Wait -WindowStyle Hidden
        if ($taskLocked.ExitCode -eq 0) { throw 'Locked resource was incorrectly accepted.' }
        if ((Get-FileHash -LiteralPath (Join-Path $taskInstall 'snowdown.exe')).Hash -ne $taskBeforeLock) { throw 'Installer partially overwrote a locked installation.' }
    } finally { $taskLock.Dispose() }
    Write-Output 'Cross-version v0.1.10 -> current passed: old executable/registry/no manifest, protected data, locked component rejects before overwrite.'
    # A harmless console fixture holds the target executable. Neither frontend nor
    # NSIS backend may terminate it to complete an installation.
    $taskHolderSource = Join-Path $taskQaRoot 'Holder.cs'
    [IO.File]::WriteAllText($taskHolderSource,'class Holder { static void Main() { System.Threading.Thread.Sleep(60000); } }')
    $taskHolderExe = Join-Path $taskQaRoot 'holder.exe'
    & (Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe') /nologo /target:exe /platform:x64 "/out:$taskHolderExe" $taskHolderSource
    if ($LASTEXITCODE -ne 0) { throw 'QA active-app fixture compilation failed.' }
    $taskApp = Join-Path $taskInstall 'snowdown.exe'
    $taskOriginal = Join-Path $taskQaRoot 'original-snowdown.exe'
    Copy-Item -LiteralPath $taskApp -Destination $taskOriginal
    Copy-Item -LiteralPath $taskHolderExe -Destination $taskApp -Force
    $taskHolder = Start-Process -FilePath $taskApp -PassThru -WindowStyle Hidden
    try {
        $taskStart = New-Object Diagnostics.ProcessStartInfo
        $taskStart.FileName = $taskPayload
        $taskStart.Arguments = "/S /NS /D=$taskInstall"
        $taskStart.UseShellExecute = $false
        $taskStart.CreateNoWindow = $true
        $taskBusyStage = Join-Path $taskQaRoot 'busy-stage'
        $taskStart.EnvironmentVariables['SNOWDOWN_SETUP_STATUS'] = $taskBusyStage
        $taskBackend = [Diagnostics.Process]::Start($taskStart)
        $taskBackend.WaitForExit()
        if ($taskBackend.ExitCode -eq 0 -or (Get-Content -LiteralPath $taskBusyStage -Raw) -ne 'running') { throw 'Backend did not reject the active client.' }
        if ($taskHolder.HasExited) { throw 'Backend terminated the active client.' }
        $taskBackend.Dispose()
        $taskBlockedWrapper = Start-Process -FilePath $taskWrapper -ArgumentList @('/S',"`"/D=$taskInstall`"") -PassThru -Wait -WindowStyle Hidden
        if ($taskBlockedWrapper.ExitCode -eq 0 -or $taskHolder.HasExited) { throw 'Frontend did not preserve the active client.' }
    }
    finally {
        if (-not $taskHolder.HasExited) { $taskHolder.Kill(); $taskHolder.WaitForExit() }
        $taskHolder.Dispose()
        Copy-Item -LiteralPath $taskOriginal -Destination $taskApp -Force
    }
    $taskSentinel = Join-Path $taskInstall 'user-kept.txt'
    [IO.File]::WriteAllText($taskSentinel,'preserve this file during reinstall')
    $taskStage = Join-Path $taskQaRoot 'backend-stage'
    $taskStart = New-Object Diagnostics.ProcessStartInfo
    $taskStart.FileName = $taskPayload
    $taskStart.Arguments = "/S /NS /D=$taskInstall"
    $taskStart.UseShellExecute = $false
    $taskStart.CreateNoWindow = $true
    $taskStart.EnvironmentVariables['SNOWDOWN_SETUP_STATUS'] = $taskStage
    $taskBackend = [Diagnostics.Process]::Start($taskStart)
    $taskBackend.WaitForExit()
    if ($taskBackend.ExitCode -ne 0) { throw 'QA reinstall failed.' }
    if ((Get-Content -LiteralPath $taskStage -Raw) -ne 'complete') { throw 'Backend completion phase missing.' }
    if (-not (Test-Path -LiteralPath $taskSentinel)) { throw 'Reinstall removed an unrelated user file.' }
    $taskBackend.Dispose()
    # A harmless fixture verifies automatic restart without opening the user's real client.
    $taskAutoSource = Join-Path $taskQaRoot 'AutoApp.cs'
    [IO.File]::WriteAllText($taskAutoSource,'class AutoApp { static void Main(string[] args) { if(args.Length>0){System.Threading.Thread.Sleep(1200);return;}System.IO.File.WriteAllText(System.IO.Path.Combine(System.AppDomain.CurrentDomain.BaseDirectory,"auto-launched.txt"),"launched"); } }')
    $taskAutoSourceDir = Join-Path $taskQaRoot 'auto-source'
    New-Item -ItemType Directory -Path $taskAutoSourceDir | Out-Null
    $taskAutoExe = Join-Path $taskAutoSourceDir 'snowdown.exe'
    & (Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe') /nologo /target:exe /platform:x64 "/out:$taskAutoExe" $taskAutoSource
    if ($LASTEXITCODE -ne 0) { throw 'Auto fixture compilation failed.' }
    $taskMainMatch = [regex]::Match($taskScript,'(?m)^!define MAINBINARYSRCPATH "([^\r\n]+)"')
    if (-not $taskMainMatch.Success) { throw 'Missing main executable definition.' }
    $taskAutoPayload = Join-Path $taskQaRoot 'auto-payload.exe'
    $taskAutoScript = $taskScript.Replace($taskMainMatch.Groups[1].Value,$taskAutoExe).Replace($taskPayload,$taskAutoPayload)
    $taskAutoNsi = Join-Path $taskQaRoot 'auto.nsi'
    $taskAutoScript | Set-Content -LiteralPath $taskAutoNsi -Encoding utf8
    & $taskCompiler /V2 "/X!addincludedir `"$taskGenerated`"" $taskAutoNsi
    if ($LASTEXITCODE -ne 0) { throw 'Auto payload compilation failed.' }
    $taskAutoWrapper = Join-Path $taskQaRoot 'auto-wrapper.exe'
    & (Join-Path $taskRoot 'scripts\build-installer.ps1') -SkipTests -PayloadPath $taskAutoPayload -OutputPath $taskAutoWrapper -ProgramPath $taskAutoExe
    $taskPrevious = Join-Path $taskInstall '.snowdown-program-files.v1'
    $taskOld = Join-Path $taskInstall 'resources\obsolete.dll'
    [IO.File]::WriteAllText($taskOld,'owned obsolete binary')
    $taskOldHash = (Get-FileHash -LiteralPath $taskOld).Hash.ToLowerInvariant()
    [IO.File]::AppendAllText($taskPrevious,$taskOldHash+'|resources/obsolete.dll'+[char]10)
    foreach ($taskProtected in @('Cookies','settings.json','tasks.db','download.mp4')) { [IO.File]::WriteAllText((Join-Path $taskInstall $taskProtected),'preserve-'+$taskProtected) }
    $taskWaiting = Start-Process -FilePath $taskAutoExe -ArgumentList '--hold' -PassThru -WindowStyle Hidden
    $taskOldKey = $taskUserRegistry.OpenSubKey($taskRegistry,$true)
    $taskOldKey.SetValue('DisplayVersion','0.2.4'); $taskOldKey.Dispose()
    $taskAutoResult = Start-Process -FilePath $taskAutoWrapper -ArgumentList @('--auto-update',"`"--update-dir=$taskInstall`"","--wait-pid=$($taskWaiting.Id)") -PassThru -Wait -WindowStyle Hidden
    if ($taskAutoResult.ExitCode -ne 0) { throw "Auto overwrite failed: $($taskAutoResult.ExitCode)" }
    for ($taskWait=0; $taskWait -lt 30 -and -not (Test-Path -LiteralPath (Join-Path $taskInstall 'auto-launched.txt')); $taskWait++) { Start-Sleep -Milliseconds 100 }
    if (-not (Test-Path -LiteralPath (Join-Path $taskInstall 'auto-launched.txt'))) { throw 'Updated program was not restarted.' }
    if ((Get-FileHash -LiteralPath $taskApp).Hash -ne (Get-FileHash -LiteralPath $taskAutoExe).Hash) { throw 'Auto update did not replace the program.' }
    if (Test-Path -LiteralPath $taskOld) { throw 'Obsolete owned program file was not removed.' }
    foreach ($taskProtected in @('Cookies','settings.json','tasks.db','download.mp4')) { if ([IO.File]::ReadAllText((Join-Path $taskInstall $taskProtected)) -ne 'preserve-'+$taskProtected) { throw 'Auto update changed protected data.' } }
    Write-Output 'Auto-update passed: wait for old process, silent overwrite, restart, manifest cleanup, preserved cookies/settings/tasks/downloads.'
    $taskResult = Start-Process -FilePath $taskUninstall -ArgumentList "/S _?=$taskInstall" -PassThru -Wait -WindowStyle Hidden
    if ($taskResult.ExitCode -ne 0) { throw 'QA uninstall failed.' }
    if (Test-Path -LiteralPath (Join-Path $taskInstall 'snowdown.exe')) { throw 'Uninstaller left the application binary.' }
    if (Test-Path -LiteralPath (Join-Path $taskInstall 'resources\ffmpeg.exe')) { throw 'Uninstaller left media components.' }
    if (-not (Test-Path -LiteralPath $taskSentinel)) { throw 'Uninstaller removed an unrelated user file.' }
    $taskKey = $taskUserRegistry.OpenSubKey($taskRegistry)
    if ($taskKey) { $taskKey.Dispose(); throw 'QA uninstall registry entry was not removed.' }
    Write-Output 'Installer integration passed: Unicode path, wrapper extraction, file integrity, registration, active-client protection, reinstall, real phases, uninstall, preserved user file.'
}
finally {
    if (Test-Path -LiteralPath (Join-Path $taskInstall 'snowdown.exe')) {
        if (Test-Path -LiteralPath $taskUninstall) { Start-Process -FilePath $taskUninstall -ArgumentList "/S _?=$taskInstall" -PassThru -Wait -WindowStyle Hidden | Out-Null }
    }
    $taskUserRegistry.DeleteSubKeyTree($taskMakerRegistry,$false)
    $taskUserRegistry.Dispose()
    # Only this run's verified, unique workspace directory is removed.
    if ([IO.Path]::GetFullPath($taskQaRoot).StartsWith($taskRoot + [IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)) { Remove-Item -LiteralPath $taskQaRoot -Recurse -Force }
}
