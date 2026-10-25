param([string]$SevenZipPath = $env:SNOWDOWN_7ZIP)
$ErrorActionPreference = 'Stop'
$taskRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$taskQaRoot = Join-Path $taskRoot ('.tools\manifest-regression-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $taskQaRoot | Out-Null
try {
    # Use the shipped artifact, not a recompiled NSIS script referencing the
    # restored standalone EXE. The latter hid this production regression.
    $taskAssembly = [Reflection.Assembly]::LoadFile((Join-Path $taskRoot 'release\SnowDown_0.2.5_x64-setup.exe'))
    $taskReader = New-Object IO.StreamReader($taskAssembly.GetManifestResourceStream('SnowDown.Manifest'))
    try { $taskOldManifest = $taskReader.ReadToEnd() } finally { $taskReader.Dispose() }
    $taskPayload = Join-Path $taskQaRoot 'payload.exe'
    $taskSource = $taskAssembly.GetManifestResourceStream('SnowDown.Payload')
    $taskOutput = [IO.File]::Create($taskPayload)
    try { $taskSource.CopyTo($taskOutput) } finally { $taskOutput.Dispose(); $taskSource.Dispose() }
    $taskManifestPath = Join-Path $taskQaRoot 'program-files.v1'
    & (Join-Path $taskRoot 'scripts\write-installer-manifest.ps1') -PayloadPath $taskPayload -OutputPath $taskManifestPath -SevenZipPath $SevenZipPath
    $taskOld = @{}; $taskCurrent = @{}
    foreach ($taskLine in $taskOldManifest.Split([char]10)) { $taskParts=$taskLine.Trim().Split('|'); if($taskParts.Count -eq 2){$taskOld[$taskParts[1]]=$taskParts[0]} }
    foreach ($taskLine in [IO.File]::ReadAllLines($taskManifestPath)) { $taskParts=$taskLine.Trim().Split('|'); if($taskParts.Count -eq 2){$taskCurrent[$taskParts[1]]=$taskParts[0]} }
    if ($taskCurrent.Count -ne 17 -or $taskCurrent.Count -ne $taskOld.Count) { throw 'Archived payload file inventory changed.' }
    if ($taskOld['snowdown.exe'] -ne 'ab500f8aefaff0773254b3b153b5e9c85875e3b91df3166eadadf83c05ab155c') { throw 'Expected the original faulty v0.2.5 manifest.' }
    if ($taskCurrent['snowdown.exe'] -ne '74439de0e84156cdad4c9cec66333a1be1122fb5ed3ad90ba23159ff9c9cfc70') { throw 'Manifest did not hash the actual NSIS executable.' }
    foreach ($taskName in $taskOld.Keys) {
        if ($taskName -ne 'snowdown.exe' -and $taskCurrent[$taskName] -ne $taskOld[$taskName]) { throw "Unexpected resource hash: $taskName" }
    }
    Write-Output 'Archived v0.2.5 regression passed: actual NSIS EXE digest, all 16 resource digests, 17 owned files.'
}
finally {
    $taskResolved = [IO.Path]::GetFullPath($taskQaRoot)
    if ($taskResolved.StartsWith($taskRoot + [IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)) { Remove-Item -LiteralPath $taskResolved -Recurse -Force }
}
