<#
.SYNOPSIS
  Build and run the experimental GPUI desktop together with its matching
  Alera CLI sidecar. Classic Alera and its Release folder remain unchanged.
.EXAMPLE
  .\tool\dev\run_alera_native_windows.ps1 -Offline
.EXAMPLE
  .\tool\dev\run_alera_native_windows.ps1 -Workspace 'C:\Projects\Alera'
#>
[CmdletBinding()]
param(
    [string]$Workspace = '',
    [switch]$Offline,
    [switch]$NoLaunch,
    [string]$TargetDir = ''
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
if ([string]::IsNullOrWhiteSpace($TargetDir)) {
    $TargetDir = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'AleraNative\build'
}
$target = [IO.Path]::GetFullPath($TargetDir)
if ([string]::IsNullOrWhiteSpace($Workspace)) {
    $Workspace = $repo
}
$workspacePath = (Resolve-Path -LiteralPath $Workspace -ErrorAction Stop).Path
if (-not [IO.Directory]::Exists($workspacePath)) {
    throw "Workspace is not a directory: $workspacePath"
}

# Use a single dedicated target directory so the Native executable always
# discovers the *matching* CLI by its sibling path. Never copy an older
# Classic Release CLI; it may not implement terminal --no-start/create.
$originalTarget = $env:CARGO_TARGET_DIR
$env:CARGO_TARGET_DIR = $target
try {
    $flags = @('--locked')
    if ($Offline) { $flags += '--offline' }
    & cargo build --manifest-path (Join-Path $repo 'rust\Cargo.toml') -p alera-cli @flags
    if ($LASTEXITCODE -ne 0) { throw "Alera CLI build failed: $LASTEXITCODE" }
    & cargo build --manifest-path (Join-Path $repo 'desktop_gpui\Cargo.toml') @flags
    if ($LASTEXITCODE -ne 0) { throw "Alera GPUI build failed: $LASTEXITCODE" }
    $exe = Join-Path $target 'debug\alera-gpui-desktop.exe'
    $cli = Join-Path $target 'debug\alera.exe'
    if (-not [IO.File]::Exists($exe) -or -not [IO.File]::Exists($cli)) {
        throw 'Expected sibling Native and Alera CLI executables were not built'
    }
    $terminalHelp = (& $cli terminal --help) -join [Environment]::NewLine
    if ($LASTEXITCODE -ne 0 -or -not $terminalHelp.Contains('--no-start') -or
        -not $terminalHelp.Contains('workspaces') -or
        -not $terminalHelp.Contains('create')) {
        throw 'Alera CLI does not provide the Native Host terminal contract'
    }
    Write-Output "Native: $exe"
    Write-Output "CLI: $cli"
    if (-not $NoLaunch) {
        Start-Process -FilePath $exe -WorkingDirectory $workspacePath -ArgumentList @($workspacePath)
    }
} finally {
    if ($null -eq $originalTarget) {
        Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue
    } else {
        $env:CARGO_TARGET_DIR = $originalTarget
    }
}
