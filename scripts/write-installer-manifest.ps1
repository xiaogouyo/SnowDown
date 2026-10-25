param(
    [Parameter(Mandatory=$true)][string]$PayloadPath,
    [Parameter(Mandatory=$true)][string]$OutputPath,
    [string]$SevenZipPath = $env:SNOWDOWN_7ZIP
)
$ErrorActionPreference = 'Stop'
if (-not $SevenZipPath) {
    $taskCommand = Get-Command 7z.exe,7zz.exe -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($taskCommand) { $SevenZipPath = $taskCommand.Source }
    elseif (Test-Path -LiteralPath (Join-Path $env:ProgramFiles '7-Zip\7z.exe')) { $SevenZipPath = Join-Path $env:ProgramFiles '7-Zip\7z.exe' }
}
if (-not $SevenZipPath -or -not (Test-Path -LiteralPath $SevenZipPath)) {
    throw '7-Zip is required to hash the actual NSIS payload. Install 7-Zip or set SNOWDOWN_7ZIP to 7z.exe.'
}
$taskRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$taskExtractRoot = Join-Path $taskRoot '.tools\installer-build'
$taskExtract = Join-Path $taskExtractRoot ('manifest-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $taskExtract | Out-Null
try {
    & $SevenZipPath x ([IO.Path]::GetFullPath($PayloadPath)) "-o$taskExtract" -y -bso0 -bsp0
    if ($LASTEXITCODE -ne 0) { throw 'Cannot extract the NSIS payload for installed-file verification.' }
    $taskOwnedFiles = @('snowdown.exe')
    $taskExtensionRoot = Join-Path $taskExtract 'resources\browser-extension'
    if (-not (Test-Path -LiteralPath (Join-Path $taskExtensionRoot 'manifest.json'))) { throw 'The NSIS payload is missing the browser extension.' }
    $taskResourceRoot = Join-Path $taskExtract 'resources'
    $taskOwnedFiles += @(Get-ChildItem -LiteralPath $taskResourceRoot -Recurse -File | Sort-Object FullName | ForEach-Object { 'resources/' + $_.FullName.Substring($taskResourceRoot.Length+1).Replace('\','/') })
    $taskManifestLines = @('SnowDown-owned-files-v1')
    foreach ($taskOwnedFile in $taskOwnedFiles) {
        $taskSource = Join-Path $taskExtract $taskOwnedFile
        $taskManifestLines += (Get-FileHash -LiteralPath $taskSource -Algorithm SHA256).Hash.ToLowerInvariant() + '|' + $taskOwnedFile
    }
    [IO.File]::WriteAllText([IO.Path]::GetFullPath($OutputPath),($taskManifestLines -join "`n") + "`n",(New-Object Text.UTF8Encoding($false)))
}
finally {
    $taskResolved = [IO.Path]::GetFullPath($taskExtract)
    if ($taskResolved.StartsWith([IO.Path]::GetFullPath($taskExtractRoot) + [IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)) {
        Remove-Item -LiteralPath $taskResolved -Recurse -Force
    }
}
