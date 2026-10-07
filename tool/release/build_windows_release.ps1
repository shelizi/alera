[CmdletBinding()]
param(
    [string]$FlutterPath,
    [string]$CargoScratchDirectory,
    [switch]$NoSccache,
    [switch]$CheckOnly
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if (-not [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform(
    [System.Runtime.InteropServices.OSPlatform]::Windows
)) {
    throw 'This script only supports Windows.'
}

$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..')).Path
$requiredRustToolchain = '1.98.0'

function Write-Step([string]$Message) {
    Write-Host "`n==> $Message" -ForegroundColor Cyan
}

function Get-EnvironmentValue([string]$Name) {
    $item = Get-Item -LiteralPath "Env:$Name" -ErrorAction SilentlyContinue
    if ($null -eq $item) {
        return $null
    }
    return [string]$item.Value
}

function Restore-EnvironmentValue([string]$Name, [string]$Value) {
    if ($null -eq $Value) {
        Remove-Item -LiteralPath "Env:$Name" -ErrorAction SilentlyContinue
    }
    else {
        Set-Item -LiteralPath "Env:$Name" -Value $Value
    }
}

function Resolve-FlutterExecutable {
    if ($FlutterPath) {
        return (Resolve-Path -LiteralPath $FlutterPath -ErrorAction Stop).Path
    }

    $localFlutter = Join-Path $repoRoot '.tools\flutter\bin\flutter.bat'
    if (Test-Path -LiteralPath $localFlutter -PathType Leaf) {
        return (Resolve-Path -LiteralPath $localFlutter).Path
    }

    $flutter = Get-Command flutter -ErrorAction SilentlyContinue
    if ($null -eq $flutter) {
        throw 'Flutter was not found. Pass -FlutterPath or add Flutter to PATH.'
    }
    return $flutter.Source
}

function Invoke-Checked {
    param(
        [Parameter(Mandatory = $true)][string]$FilePath,
        [Parameter(Mandatory = $false)][string[]]$Arguments = @()
    )

    & $FilePath @Arguments
    $exitCode = $LASTEXITCODE
    if ($exitCode -ne 0) {
        throw "Command failed with exit code ${exitCode}: $FilePath $($Arguments -join ' ')"
    }
}

function Resolve-CMakeExecutable {
    $cmake = Get-Command cmake.exe -ErrorAction SilentlyContinue
    if ($null -ne $cmake) {
        return $cmake.Source
    }

    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (Test-Path -LiteralPath $vswhere -PathType Leaf) {
        $installations = @(& $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath)
        foreach ($installation in $installations) {
            $candidate = Join-Path ([string]$installation).Trim() 'Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin\cmake.exe'
            if (Test-Path -LiteralPath $candidate -PathType Leaf) {
                return (Resolve-Path -LiteralPath $candidate).Path
            }
        }
    }

    throw 'cmake.exe was not found. Install the Visual Studio C++ desktop workload.'
}

function Get-CMakeCacheValue([string]$CachePath, [string]$Key) {
    if (-not (Test-Path -LiteralPath $CachePath -PathType Leaf)) {
        return $null
    }

    $line = Get-Content -LiteralPath $CachePath |
        Where-Object { $_ -match "^$([regex]::Escape($Key)):[^=]+=.*$" } |
        Select-Object -First 1
    if ($null -eq $line) {
        return $null
    }
    return (($line -split '=', 2)[1]).Trim()
}

function Ensure-ReleaseCMakeConfiguration {
    $cmakeBuildDirectory = Join-Path $repoRoot 'build\windows\x64'
    $cachePath = Join-Path $cmakeBuildDirectory 'CMakeCache.txt'
    if (-not (Test-Path -LiteralPath $cachePath -PathType Leaf)) {
        Write-Host 'No existing CMake cache; Flutter will configure the Release target.'
        return
    }

    $generator = Get-CMakeCacheValue $cachePath 'CMAKE_GENERATOR'
    if ([string]::IsNullOrWhiteSpace($generator)) {
        throw "Could not read CMAKE_GENERATOR from $cachePath"
    }
    $platform = Get-CMakeCacheValue $cachePath 'CMAKE_GENERATOR_PLATFORM'
    if ([string]::IsNullOrWhiteSpace($platform) -and $generator -like 'Visual Studio*') {
        $platform = 'x64'
    }

    Write-Step "Ensuring Release CMake configuration ($generator)"
    $cmake = Resolve-CMakeExecutable
    $arguments = @('-S', (Join-Path $repoRoot 'windows'), '-B', $cmakeBuildDirectory, '-G', $generator)
    if (-not [string]::IsNullOrWhiteSpace($platform)) {
        $arguments += @('-A', $platform)
    }
    Invoke-Checked -FilePath $cmake -Arguments $arguments

    $releaseProject = Join-Path $cmakeBuildDirectory 'runner\Alera.vcxproj'
    if (-not (Test-Path -LiteralPath $releaseProject -PathType Leaf)) {
        throw "Release CMake configuration did not generate $releaseProject. Check ALERA_FLAVOR and rerun."
    }
}

function Assert-ReleaseInstallManifest {
    $manifestPath = Join-Path $repoRoot 'build\windows\x64\cmake_install.cmake'
    if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
        throw "CMake install manifest was not generated: $manifestPath"
    }

    $manifest = Get-Content -LiteralPath $manifestPath -Raw
    if ($manifest -notmatch 'runner[/\\]Release[/\\]Alera\.exe') {
        throw 'The active CMake install target is not the Windows Release Alera.exe.'
    }
    if ($manifest -match 'runner[/\\]Release[/\\]alera-dev\.exe') {
        throw 'The active CMake install target still points at alera-dev.exe.'
    }
}

function Assert-FileAvailable([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return
    }

    $stream = $null
    try {
        $stream = [System.IO.File]::Open(
            $Path,
            [System.IO.FileMode]::Open,
            [System.IO.FileAccess]::ReadWrite,
            [System.IO.FileShare]::None
        )
    }
    catch {
        throw "Build output is in use: $Path. Close Alera and its runtime host before rebuilding."
    }
    finally {
        if ($null -ne $stream) {
            $stream.Dispose()
        }
    }
}

if ($env:CI -ne 'true') {
    $prepareStorage = Join-Path $repoRoot 'tool\development\prepare_windows_build_storage.ps1'
    if ([string]::IsNullOrWhiteSpace($CargoScratchDirectory)) {
        & $prepareStorage -RepoRoot $repoRoot -CheckOnly:$CheckOnly
        $CargoScratchDirectory = $env:ALERA_CARGOKIT_TEMP_DIR
    }
    else {
        & $prepareStorage -RepoRoot $repoRoot -CargoScratchDirectory $CargoScratchDirectory -CheckOnly:$CheckOnly
    }
}
elseif ([string]::IsNullOrWhiteSpace($CargoScratchDirectory)) {
    $CargoScratchDirectory = if ($env:ALERA_CARGOKIT_TEMP_DIR) {
        $env:ALERA_CARGOKIT_TEMP_DIR
    }
    else {
        $ciDriveRoot = $env:SystemDrive
        if ([string]::IsNullOrWhiteSpace($ciDriveRoot)) {
            $ciDriveRoot = [IO.Path]::GetPathRoot($repoRoot)
        }
        Join-Path $ciDriveRoot 'c'
    }
}
if ([string]::IsNullOrWhiteSpace($CargoScratchDirectory)) {
    throw 'Cargo scratch directory was not resolved.'
}
$CargoScratchDirectory = [System.IO.Path]::GetFullPath($CargoScratchDirectory)
$flutter = Resolve-FlutterExecutable
$rustup = Get-Command rustup.exe -ErrorAction SilentlyContinue
if ($null -eq $rustup) {
    throw 'rustup.exe was not found. Install rustup and the pinned Alera Rust toolchain.'
}
$ninja = Get-Command ninja.exe -ErrorAction SilentlyContinue
if ($null -eq $ninja) {
    throw 'ninja.exe was not found. Install Ninja or run tool/development/setup_windows.ps1.'
}

$scratchNative = Join-Path $CargoScratchDirectory 'n'
$scratchSidecar = Join-Path $CargoScratchDirectory 'cli'
New-Item -ItemType Directory -Path $CargoScratchDirectory, $scratchNative, $scratchSidecar -Force | Out-Null

$lockPath = Join-Path $repoRoot 'build\windows-release-build.lock'
New-Item -ItemType Directory -Path (Split-Path -Parent $lockPath) -Force | Out-Null
$lockStream = $null
try {
    $lockStream = [System.IO.File]::Open(
        $lockPath,
        [System.IO.FileMode]::OpenOrCreate,
        [System.IO.FileAccess]::ReadWrite,
        [System.IO.FileShare]::None
    )
}
catch {
    throw "Another Windows Release build is already running for this worktree: $lockPath"
}

$environmentNames = @(
    'ALERA_FLAVOR',
    'ALERA_CARGOKIT_TEMP_DIR',
    'RUSTUP_TOOLCHAIN',
    'RUSTC_WRAPPER',
    'GGML_CCACHE'
)
$oldEnvironment = @{}
foreach ($name in $environmentNames) {
    $oldEnvironment[$name] = Get-EnvironmentValue $name
}

$startedAt = Get-Date
try {
    Write-Step 'Checking the pinned Rust toolchain'
    $rustVersion = (& $rustup.Source run $requiredRustToolchain rustc --version 2>&1 | Out-String).Trim()
    $rustExitCode = $LASTEXITCODE
    if ($rustExitCode -ne 0) {
        throw "Rust toolchain $requiredRustToolchain is unavailable: $rustVersion"
    }
    Write-Host $rustVersion
    Write-Host "Ninja: $($ninja.Source)"
    Write-Host "Cargo scratch: $CargoScratchDirectory"

    $env:ALERA_FLAVOR = 'release'
    $env:ALERA_CARGOKIT_TEMP_DIR = $CargoScratchDirectory
    $env:RUSTUP_TOOLCHAIN = $requiredRustToolchain
    $env:GGML_CCACHE = 'OFF'

    $sccache = Get-Command sccache.exe -ErrorAction SilentlyContinue
    if ($NoSccache) {
        if ((Get-EnvironmentValue 'RUSTC_WRAPPER') -eq 'sccache') {
            Remove-Item -LiteralPath 'Env:RUSTC_WRAPPER' -ErrorAction SilentlyContinue
        }
        Write-Host 'Rust compiler cache: disabled by -NoSccache'
    }
    elseif ($null -ne $sccache) {
        $env:RUSTC_WRAPPER = $sccache.Source
        & $sccache.Source --start-server 2>$null | Out-Null
        Write-Host "Rust compiler cache: $($sccache.Source)"
    }
    else {
        Write-Host 'Rust compiler cache: sccache not found; reusing Cargo build artifacts'
    }

    if ($CheckOnly) {
        Write-Host 'Check only: environment is ready for an incremental Windows Release build.' -ForegroundColor Green
        return
    }

    Ensure-ReleaseCMakeConfiguration
    Assert-ReleaseInstallManifest

    $releaseDir = Join-Path $repoRoot 'build\windows\x64\runner\Release'
    Assert-FileAvailable (Join-Path $releaseDir 'Alera.exe')
    Assert-FileAvailable (Join-Path $releaseDir 'resources\alera\alera.exe')

    Write-Step 'Building Windows Release incrementally'
    Write-Host 'The script intentionally does not run flutter clean; Flutter, CMake, Cargo, and Rust compiler caches are reused.'
    Invoke-Checked -FilePath $flutter -Arguments @(
        'build',
        'windows',
        '--release',
        '--dart-define=ALERA_FLAVOR=release'
    )

    Assert-ReleaseInstallManifest
    $appPath = Join-Path $releaseDir 'Alera.exe'
    $sidecarPath = Join-Path $releaseDir 'resources\alera\alera.exe'
    foreach ($path in @($appPath, $sidecarPath)) {
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            throw "Expected Release output was not produced: $path"
        }
    }

    $elapsed = (Get-Date) - $startedAt
    Write-Host "Windows Release build completed in $([int]$elapsed.TotalMinutes)m $($elapsed.Seconds)s." -ForegroundColor Green
    Write-Host 'CMake target: Release/Alera.exe (verified from cmake_install.cmake)'
    Write-Host "App:     $appPath"
    Write-Host "Sidecar: $sidecarPath"
}
finally {
    foreach ($name in $environmentNames) {
        Restore-EnvironmentValue $name $oldEnvironment[$name]
    }

    if ($null -ne $lockStream) {
        $lockStream.Dispose()
    }
    Remove-Item -LiteralPath $lockPath -Force -ErrorAction SilentlyContinue
}
