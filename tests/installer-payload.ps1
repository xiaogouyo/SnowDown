param(
    [Parameter(Mandatory=$true)][string]$InstallerPath,
    [string]$SevenZipPath = $env:SNOWDOWN_7ZIP
)
$ErrorActionPreference = 'Stop'
$taskRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$taskQaRoot = [IO.Path]::GetFullPath((Join-Path $taskRoot ('.tools\payload-qa-' + [Guid]::NewGuid().ToString('N'))))
if (-not $taskQaRoot.StartsWith($taskRoot + [IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)) { throw 'QA path escaped workspace.' }
if (-not $SevenZipPath) {
    $taskCommand = Get-Command 7z.exe,7zz.exe -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($taskCommand) { $SevenZipPath = $taskCommand.Source }
    elseif (Test-Path -LiteralPath (Join-Path $env:ProgramFiles '7-Zip\7z.exe')) { $SevenZipPath = Join-Path $env:ProgramFiles '7-Zip\7z.exe' }
}
if (-not $SevenZipPath) { throw '7-Zip is required for production payload verification.' }
New-Item -ItemType Directory -Path $taskQaRoot | Out-Null
try {
    # Read the exact distributable; never recompile its NSIS script or execute it.
    $taskAssembly = [Reflection.Assembly]::LoadFile([IO.Path]::GetFullPath($InstallerPath))
    $taskReader = New-Object IO.StreamReader($taskAssembly.GetManifestResourceStream('SnowDown.Manifest'))
    try { $taskManifest = $taskReader.ReadToEnd() } finally { $taskReader.Dispose() }
    $taskPayload = Join-Path $taskQaRoot 'payload.exe'
    $taskSource = $taskAssembly.GetManifestResourceStream('SnowDown.Payload')
    $taskOutput = [IO.File]::Create($taskPayload)
    try { $taskSource.CopyTo($taskOutput) } finally { $taskOutput.Dispose(); $taskSource.Dispose() }
    $taskFlags = [Reflection.BindingFlags]'Static,NonPublic'
    $taskBuildInfo = $taskAssembly.GetType('SnowDown.Setup.BuildInfo',$true)
    $taskExpected = $taskBuildInfo.GetField('PayloadHash',$taskFlags).GetRawConstantValue()
    if ((Get-FileHash -LiteralPath $taskPayload).Hash.ToLowerInvariant() -ne $taskExpected) { throw 'Embedded payload SHA256 differs from installer metadata.' }
    $taskExtract = Join-Path $taskQaRoot 'files'
    & $SevenZipPath x $taskPayload "-o$taskExtract" -y -bso0 -bsp0
    if ($LASTEXITCODE -ne 0) { throw 'Cannot extract production payload.' }
    $taskCore = $taskAssembly.GetType('SnowDown.Setup.InstallerCore',$true)
    $taskCore.GetField('LogPath',$taskFlags).SetValue($null,(Join-Path $taskQaRoot 'verification.log'))
    $taskVerifyArguments = New-Object object[] 2
    $taskVerifyArguments[0] = [string]$taskExtract
    $taskVerifyArguments[1] = [string]$taskManifest
    $taskCore.GetMethod('VerifyManifest',$taskFlags).Invoke($null,$taskVerifyArguments) | Out-Null
    $taskEntries = @($taskManifest.Split([char]10) | Where-Object { $_ -match '^[a-f0-9]{64}\|' })
    $taskResourceCount = @(Get-ChildItem -LiteralPath (Join-Path $taskExtract 'resources') -Recurse -File).Count
    if ($taskEntries.Count -ne 1 + $taskResourceCount) { throw 'Production manifest does not cover every packaged resource and main executable.' }
    $taskVersion = $taskBuildInfo.GetField('Version',$taskFlags).GetRawConstantValue()
    if ((Get-Item -LiteralPath (Join-Path $taskExtract 'snowdown.exe')).VersionInfo.ProductVersion -ne $taskVersion) { throw 'Installer and packaged application versions differ.' }
    $taskExtension = Get-Content -LiteralPath (Join-Path $taskExtract 'resources/browser-extension/manifest.json') -Raw -Encoding UTF8 | ConvertFrom-Json
    [PSCustomObject]@{
        ProductionPayloadVerified = $true
        InstallerVersion = $taskVersion
        OwnedFiles = $taskEntries.Count
        ExtensionVersion = $taskExtension.version
        PackagedExecutableSHA256 = (Get-FileHash -LiteralPath (Join-Path $taskExtract 'snowdown.exe')).Hash.ToLowerInvariant()
    } | ConvertTo-Json -Compress
}
finally {
    if ([IO.Path]::GetFullPath($taskQaRoot).StartsWith($taskRoot + [IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)) { Remove-Item -LiteralPath $taskQaRoot -Recurse -Force }
}
