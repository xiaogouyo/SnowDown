param([switch]$SkipTests, [string]$PayloadPath, [string]$OutputPath)
$ErrorActionPreference = 'Stop'
$taskRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$taskVersion = (Get-Content -LiteralPath (Join-Path $taskRoot 'package.json') -Raw | ConvertFrom-Json).version
if ($taskVersion -notmatch '^\d+\.\d+\.\d+$') { throw 'Expected a numeric release version.' }
$taskFramework = Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319'
$taskCompiler = Join-Path $taskFramework 'csc.exe'
$taskBuild = Join-Path $taskRoot '.tools\installer-build'
$taskBundle = Join-Path $taskRoot 'src-tauri\target\release\bundle\snowdown'
$taskPayload = Join-Path $taskRoot "src-tauri\target\release\bundle\nsis\SnowDown_$($taskVersion)_x64-setup.exe"
if ($PayloadPath) { $taskPayload = [IO.Path]::GetFullPath($PayloadPath) }
if (-not (Test-Path -LiteralPath $taskCompiler)) { throw 'The Windows .NET Framework compiler is required.' }
if (-not (Test-Path -LiteralPath $taskPayload)) { throw 'Build the Tauri NSIS payload first.' }
New-Item -ItemType Directory -Force -Path $taskBuild,$taskBundle | Out-Null
$taskHashAlgorithm = [Security.Cryptography.SHA256]::Create()
$taskHashStream = [IO.File]::OpenRead($taskPayload)
try { $taskPayloadHash = [BitConverter]::ToString($taskHashAlgorithm.ComputeHash($taskHashStream)).Replace('-','').ToLowerInvariant() }
finally { $taskHashStream.Dispose(); $taskHashAlgorithm.Dispose() }
$taskInfo = Join-Path $taskBuild 'BuildInfo.cs'
@"
using System.Reflection;
[assembly: AssemblyTitle("SnowDown Installer")]
[assembly: AssemblyProduct("SnowDown")]
[assembly: AssemblyVersion("$taskVersion.0")]
[assembly: AssemblyFileVersion("$taskVersion.0")]
namespace SnowDown.Setup { internal static class BuildInfo {
internal const string Version = "$taskVersion";
internal const string PayloadHash = "$taskPayloadHash";
} }
"@ | Set-Content -LiteralPath $taskInfo -Encoding utf8
$taskCore = Join-Path $taskRoot 'installer\InstallerCore.cs'
$taskManifest = Join-Path $taskBuild 'program-files.v1'
# Tauri patches the executable's bundle-type marker while building NSIS, then
# restores target/release/snowdown.exe. Only the payload contains the final bytes.
& (Join-Path $PSScriptRoot 'write-installer-manifest.ps1') -PayloadPath $taskPayload -OutputPath $taskManifest
if (-not $SkipTests) {
    $taskTests = Join-Path $taskBuild 'InstallerCoreTests.exe'
    & $taskCompiler /nologo /codepage:65001 /target:exe /platform:x64 /optimize+ "/out:$taskTests" $taskCore $taskInfo (Join-Path $taskRoot 'installer\InstallerCoreTests.cs')
    if ($LASTEXITCODE -ne 0) { throw 'Installer test compilation failed.' }
    & $taskTests
    if ($LASTEXITCODE -ne 0) { throw 'Installer core tests failed.' }
}
$taskOutput = Join-Path $taskBundle "SnowDown_$($taskVersion)_x64-setup.exe"
if ($OutputPath) { $taskOutput = [IO.Path]::GetFullPath($OutputPath) }
$taskArguments = @('/nologo', '/codepage:65001', '/target:winexe', '/platform:x64', '/optimize+', "/out:$taskOutput", "/win32manifest:$(Join-Path $taskRoot 'installer\app.manifest')", "/win32icon:$(Join-Path $taskRoot 'src-tauri\icons\icon.ico')")
foreach ($taskLibrary in @('WPF\PresentationFramework.dll', 'WPF\PresentationCore.dll', 'WPF\WindowsBase.dll', 'System.Xaml.dll', 'System.Windows.Forms.dll')) {
    $taskArguments += "/reference:$(Join-Path $taskFramework $taskLibrary)"
}
$taskArguments += @(
    "/resource:$taskPayload,SnowDown.Payload",
    "/resource:$taskManifest,SnowDown.Manifest",
    "/resource:$(Join-Path $taskRoot 'installer\Installer.xaml'),SnowDown.Layout",
    "/resource:$(Join-Path $taskRoot 'installer\assets\character.png'),SnowDown.Character",
    "/resource:$(Join-Path $taskRoot 'src-tauri\icons\icon.ico'),SnowDown.Icon",
    (Join-Path $taskRoot 'installer\Program.cs'), $taskCore, $taskInfo
)
& $taskCompiler @taskArguments
if ($LASTEXITCODE -ne 0) { throw 'Custom installer compilation failed.' }
$taskVerified = Start-Process -FilePath $taskOutput -ArgumentList '--verify-payload' -PassThru -Wait -WindowStyle Hidden
if ($taskVerified.ExitCode -ne 0) { throw 'Embedded payload integrity verification failed.' }
if (-not $SkipTests) {
    & (Join-Path $taskRoot 'tests\installer-payload.ps1') -InstallerPath $taskOutput
}
Get-Item -LiteralPath $taskOutput | Select-Object FullName,Length
