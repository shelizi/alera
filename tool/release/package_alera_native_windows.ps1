<#
.SYNOPSIS
  Create an independent, non-published Windows ZIP for Alera Native (GPUI).
  This script does not access or modify the Classic Alera installation.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$NativeExe,
    [Parameter(Mandatory = $true)][string]$CliExe,
    [Parameter(Mandatory = $true)][string]$OutputDir,
    [Parameter(Mandatory = $true)][string]$Version
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

if ($Version -notmatch '^[0-9A-Za-z][0-9A-Za-z._-]{0,63}$') {
    throw 'Version must be a bounded filename-safe string'
}
$nativePath = (Resolve-Path -LiteralPath $NativeExe -ErrorAction Stop).Path
$cliPath = (Resolve-Path -LiteralPath $CliExe -ErrorAction Stop).Path
if (-not [IO.Path]::IsPathRooted($OutputDir)) {
    throw 'OutputDir must be an absolute path'
}
if (-not [IO.File]::Exists($nativePath) -or -not [IO.File]::Exists($cliPath)) {
    throw 'Both the GPUI desktop binary and the Alera CLI sidecar are required'
}
if ([IO.Path]::GetFullPath($nativePath) -eq [IO.Path]::GetFullPath($cliPath)) {
    throw 'Native UI and CLI paths must refer to different executables'
}

$output = [IO.Path]::GetFullPath($OutputDir)
New-Item -ItemType Directory -Force -Path $output | Out-Null
$zip = Join-Path $output "AleraNative-$Version-windows-x64-preview.zip"
$shaFile = "$zip.sha256"
if ([IO.File]::Exists($zip) -or [IO.File]::Exists($shaFile)) {
    throw "Refusing to overwrite an existing Native preview archive: $zip"
}

$stage = Join-Path ([IO.Path]::GetTempPath()) "alera-native-package-$([guid]::NewGuid().ToString('N'))"
try {
    $package = Join-Path $stage 'AleraNative'
    New-Item -ItemType Directory -Force -Path $package | Out-Null
    Copy-Item -LiteralPath $nativePath -Destination (Join-Path $package 'AleraNative.exe') -ErrorAction Stop
    Copy-Item -LiteralPath $cliPath -Destination (Join-Path $package 'alera.exe') -ErrorAction Stop
    $nativeToastShortcutTool = Join-Path $PSScriptRoot 'install_alera_native_toast_shortcut.ps1'
    Copy-Item -LiteralPath $nativeToastShortcutTool -Destination (Join-Path $package 'install_alera_native_toast_shortcut.ps1') -ErrorAction Stop
    @{
        product = 'Alera Native'
        appId = 'dev.leynier.alera.native'
        executable = 'AleraNative.exe'
        version = $Version
        channel = 'native-preview'
        supported = $false
        updateMode = 'manual-preview-only'
        sharedRuntimeHost = $true
    } | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $package 'edition.json') -Encoding utf8
    @'
Alera Native (GPUI experimental preview)

Launch AleraNative.exe. This is a separate Windows application from
Alera Classic (Flutter), with a distinct executable and AppUserModelID.
Both desktop applications may run simultaneously.

The bundled alera.exe is the existing Alera CLI/Runtime Host client. Native
does not own or stop Classic's Host sessions, and both UIs may see the same
Host-owned PTYs. Do not type into the same PTY in both apps simultaneously.

Native-only UI sessions and scratch drafts use separate state. Older GPUI
previews may reuse Alera/gpui-preview to avoid hiding unsaved drafts.

Windows Toast prereq for this unpackaged Native preview (manual opt-in):
  powershell -NoProfile -File .\install_alera_native_toast_shortcut.ps1 -Mode Check
  powershell -NoProfile -File .\install_alera_native_toast_shortcut.ps1 -Mode Install -NativeExe (Resolve-Path .\AleraNative.exe).Path
  powershell -NoProfile -File .\install_alera_native_toast_shortcut.ps1 -Mode Uninstall
This opt-in tool registers ONLY the per-user Alera Native Preview shortcut
with Native's separate AppUserModelID and never modifies Alera Classic.
It does not enable GPUI production Agent notification ownership or guarantee
Windows will show Toasts; real WinRT display/click remains an H03 acceptance gate.

This ZIP is an opt-in preview, not a Classic update or production release.
The ZIP does not auto-run any installer, shortcut registration, or updater.
'@ | Set-Content -LiteralPath (Join-Path $package 'README.txt') -Encoding utf8
    Compress-Archive -LiteralPath $package -DestinationPath $zip -CompressionLevel Optimal -ErrorAction Stop
    $hash = (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLowerInvariant()
    "$hash  $([IO.Path]::GetFileName($zip))" | Set-Content -LiteralPath $shaFile -Encoding ascii
    Write-Output "Native preview package: $zip"
    Write-Output "SHA256: $hash"
} finally {
    if ([IO.Directory]::Exists($stage)) {
        Remove-Item -LiteralPath $stage -Recurse -Force -ErrorAction SilentlyContinue
    }
}
