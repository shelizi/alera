[CmdletBinding()]
param(
    [string]$RepoRoot = (Join-Path $PSScriptRoot '..\..'),
    [string]$StorageRoot,
    [string]$CargoScratchDirectory,
    [int]$MinimumFreeSpaceGB = 20,
    [switch]$CheckOnly
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if (-not [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform(
    [System.Runtime.InteropServices.OSPlatform]::Windows
)) {
    throw 'This script only supports Windows.'
}
$RepoRoot = (Resolve-Path -LiteralPath $RepoRoot).Path.TrimEnd('\')
if (-not (Test-Path -LiteralPath (Join-Path $RepoRoot 'pubspec.yaml') -PathType Leaf)) {
    throw "Not an Alera checkout: $RepoRoot"
}

$cargoConfig = Join-Path $RepoRoot '.cargo\config.toml'
$storageMetadata = Join-Path $RepoRoot '.cargo\alera-build-storage.json'
$minimumFreeBytes = [int64]$MinimumFreeSpaceGB * 1GB

function Get-LocalFixedDrive([string]$Path) {
    $fullPath = [IO.Path]::GetFullPath($Path)
    $root = [IO.Path]::GetPathRoot($fullPath)
    if ([string]::IsNullOrWhiteSpace($root)) {
        throw "Build storage path is not rooted: $Path"
    }
    $drive = [IO.DriveInfo]::new($root)
    if (-not $drive.IsReady -or $drive.DriveType -ne [IO.DriveType]::Fixed) {
        throw "Build storage must use a ready local fixed drive: $Path"
    }
    return $drive
}

function Assert-StoragePath([string]$Path) {
    $fullPath = [IO.Path]::GetFullPath($Path).TrimEnd('\')
    if ($fullPath -match '(?i)(^|[\\/])Dropbox([\\/]|$)') {
        throw "Local build storage must be outside Dropbox: $fullPath"
    }
    $drive = Get-LocalFixedDrive $fullPath
    if ($drive.AvailableFreeSpace -lt $minimumFreeBytes) {
        $freeGB = [Math]::Round($drive.AvailableFreeSpace / 1GB, 1)
        throw "Build storage drive $($drive.Name) has only ${freeGB} GB free; at least ${MinimumFreeSpaceGB} GB is required."
    }
    return $fullPath
}

function Get-ExistingCargoScratchDirectory {
    if (-not (Test-Path -LiteralPath $cargoConfig -PathType Leaf)) {
        return $null
    }
    $content = Get-Content -LiteralPath $cargoConfig -Raw
    $match = [regex]::Match($content, '(?m)^\s*target-dir\s*=\s*[''"](?<path>[^''"]+)[''"]\s*$')
    if (-not $match.Success) {
        return $null
    }
    $target = $match.Groups['path'].Value.Replace('/', '\')
    $target = [IO.Path]::GetFullPath($target)
    if ((Split-Path -Leaf $target) -ne 't') {
        return $null
    }
    return Split-Path -Parent $target
}

if ([string]::IsNullOrWhiteSpace($StorageRoot) -and $env:ALERA_BUILD_STORAGE_ROOT) {
    $StorageRoot = $env:ALERA_BUILD_STORAGE_ROOT
}
if ([string]::IsNullOrWhiteSpace($CargoScratchDirectory) -and $env:ALERA_CARGOKIT_TEMP_DIR) {
    $CargoScratchDirectory = $env:ALERA_CARGOKIT_TEMP_DIR
}

if ([string]::IsNullOrWhiteSpace($StorageRoot) -and
    [string]::IsNullOrWhiteSpace($CargoScratchDirectory)) {
    $existingScratch = Get-ExistingCargoScratchDirectory
    if ($existingScratch) {
        $CargoScratchDirectory = $existingScratch
        $existingDrive = Get-LocalFixedDrive $existingScratch
        $StorageRoot = Join-Path $existingDrive.RootDirectory.FullName 'alera-build'
    }
}

if ([string]::IsNullOrWhiteSpace($StorageRoot) -and
    [string]::IsNullOrWhiteSpace($CargoScratchDirectory)) {
    $candidates = @(
        [IO.DriveInfo]::GetDrives() |
            Where-Object {
                $_.IsReady -and
                $_.DriveType -eq [IO.DriveType]::Fixed -and
                $_.AvailableFreeSpace -ge $minimumFreeBytes
            } |
            Sort-Object AvailableFreeSpace -Descending
    )
    if ($candidates.Count -eq 0) {
        $available = @(
            [IO.DriveInfo]::GetDrives() |
                Where-Object { $_.IsReady -and $_.DriveType -eq [IO.DriveType]::Fixed } |
                ForEach-Object {
                    "$($_.Name)=$([Math]::Round($_.AvailableFreeSpace / 1GB, 1))GB"
                }
        ) -join ', '
        throw "No local fixed drive has at least ${MinimumFreeSpaceGB} GB free. Available: $available"
    }
    $selectedDrive = $candidates[0]
    $StorageRoot = Join-Path $selectedDrive.RootDirectory.FullName 'alera-build'
    $CargoScratchDirectory = Join-Path $selectedDrive.RootDirectory.FullName 'c'
    Write-Host "Selected build drive: $($selectedDrive.Name) ($([Math]::Round($selectedDrive.AvailableFreeSpace / 1GB, 1)) GB free)"
}
elseif ([string]::IsNullOrWhiteSpace($StorageRoot)) {
    $drive = Get-LocalFixedDrive $CargoScratchDirectory
    $StorageRoot = Join-Path $drive.RootDirectory.FullName 'alera-build'
}
elseif ([string]::IsNullOrWhiteSpace($CargoScratchDirectory)) {
    $drive = Get-LocalFixedDrive $StorageRoot
    $CargoScratchDirectory = Join-Path $drive.RootDirectory.FullName 'c'
}

$StorageRoot = Assert-StoragePath $StorageRoot
$CargoScratchDirectory = Assert-StoragePath $CargoScratchDirectory

$sha = [Security.Cryptography.SHA256]::Create()
try {
    $hash = $sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($RepoRoot.ToLowerInvariant()))
    $checkoutId = ([BitConverter]::ToString($hash)).Replace('-', '').Substring(0, 12).ToLowerInvariant()
}
finally {
    $sha.Dispose()
}
$checkoutStorage = Join-Path $StorageRoot "checkouts\$checkoutId"
$cargoTarget = Join-Path $CargoScratchDirectory 't'
$configText = "# Local Alera Windows build storage; generated by prepare_windows_build_storage.ps1.`n[build]`ntarget-dir = '$($cargoTarget.Replace('\', '/'))'`nincremental = false`n"
if ((Test-Path -LiteralPath $cargoConfig) -and
    (Get-Content -LiteralPath $cargoConfig -Raw).Replace("`r`n", "`n") -ne $configText) {
    throw "Preserve the existing Cargo configuration and configure external storage explicitly: $cargoConfig"
}
$redirects = [ordered]@{
    '.dart_tool' = (Join-Path $checkoutStorage 'dart-tool')
    '.tools' = (Join-Path $checkoutStorage 'tools')
    'rust\target' = $cargoTarget
    'cloud\target' = $cargoTarget
    '.prebuilt' = (Join-Path $checkoutStorage 'prebuilt')
    'windows\flutter\ephemeral' = (Join-Path $checkoutStorage 'windows-ephemeral')
    'mobile\build' = (Join-Path $checkoutStorage 'mobile-build')
    'mobile\.dart_tool' = (Join-Path $checkoutStorage 'mobile-dart-tool')
    'mobile\android\.gradle' = (Join-Path $checkoutStorage 'mobile-gradle')
    'mobile\rust_builder\.dart_tool' = (Join-Path $checkoutStorage 'mobile-rust-builder-dart-tool')
    'packages\alera_configuration\.dart_tool' = (Join-Path $checkoutStorage 'configuration-dart-tool')
    'packages\alera_configuration\build' = (Join-Path $checkoutStorage 'configuration-build')
    'rust_builder\.dart_tool' = (Join-Path $checkoutStorage 'rust-builder-dart-tool')
    'tool\release\runtime_packager\.dart_tool' = (Join-Path $checkoutStorage 'runtime-packager-dart-tool')
    'tool\release\runtime_packager\build' = (Join-Path $checkoutStorage 'runtime-packager-build')
    'landing\node_modules' = (Join-Path $checkoutStorage 'landing-node-modules')
    'landing\dist' = (Join-Path $checkoutStorage 'landing-dist')
    'landing\.astro' = (Join-Path $checkoutStorage 'landing-astro')
    'edge\node_modules' = (Join-Path $checkoutStorage 'edge-node-modules')
    'edge\.wrangler' = (Join-Path $checkoutStorage 'edge-wrangler')
    '.widget_preview\build' = (Join-Path $checkoutStorage 'preview-build')
    '.widget_preview\.dart_tool' = (Join-Path $checkoutStorage 'preview-dart-tool')
    'build' = (Join-Path $checkoutStorage 'build')
}

function Ensure-DropboxIgnored([string]$Path) {
    $ignored = Get-Content -LiteralPath $Path -Stream com.dropbox.ignored -ErrorAction SilentlyContinue
    if ([string]$ignored -eq '1') { return }
    if ($CheckOnly) {
        throw "Dropbox ignore attribute is missing: $Path"
    }
    Set-Content -LiteralPath $Path -Stream com.dropbox.ignored -Value 1
}

function Ensure-ExternalDirectory([string]$RelativePath, [string]$Destination) {
    $source = [IO.Path]::GetFullPath((Join-Path $RepoRoot $RelativePath))
    $destinationFull = [IO.Path]::GetFullPath($Destination)
    if (-not $source.StartsWith("$RepoRoot\", [StringComparison]::OrdinalIgnoreCase) -or
        $destinationFull -match '(?i)(^|[\\/])Dropbox([\\/]|$)') {
        throw "Unexpected storage mapping: $source -> $destinationFull"
    }
    [void](Get-LocalFixedDrive $destinationFull)
    $item = Get-Item -LiteralPath $source -Force -ErrorAction SilentlyContinue
    if ($null -ne $item -and ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        $target = [IO.Path]::GetFullPath([string]$item.Target).TrimEnd('\')
        if ($target -ne $destinationFull.TrimEnd('\')) {
            throw "Existing link has a different target: $source -> $target"
        }
        Ensure-DropboxIgnored $source
        Write-Host "Verified: $source -> $target"
        return
    }
    if ($CheckOnly) {
        throw "Build directory has not been redirected: $source"
    }
    if ($null -ne $item) {
        if (-not $item.PSIsContainer) {
            throw "Expected a directory: $source"
        }
        $users = @(Get-CimInstance Win32_Process | Where-Object {
            $_.Name -match '^(cargo|rustc|cmake|ninja|dart|cl|link|alera|alera-dev)\.exe$' -and
            $_.CommandLine -and $_.CommandLine.IndexOf($source, [StringComparison]::OrdinalIgnoreCase) -ge 0
        })
        if ($users.Count -gt 0) {
            throw "Build directory is in use; retry after the owning build/app exits: $source"
        }
        foreach ($lock in @(Get-ChildItem -LiteralPath $source -File -Filter '*.lock' -Force)) {
            $stream = $null
            try {
                $stream = [IO.File]::Open($lock.FullName, [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
            }
            catch {
                throw "Build directory has an active lock; retry after the owning build exits: $($lock.FullName)"
            }
            finally {
                if ($null -ne $stream) { $stream.Dispose() }
            }
        }
        $migrationRecord = Join-Path $checkoutStorage ("migration-" + $RelativePath.Replace('\', '-').Trim('.') + '.json')
        if (Test-Path -LiteralPath $destinationFull) {
            if (-not (Test-Path -LiteralPath $migrationRecord)) {
                throw "Refusing to overwrite existing storage while migrating $source to $destinationFull"
            }
            $previous = Get-Content -LiteralPath $migrationRecord -Raw | ConvertFrom-Json
            if ($previous.Source -ne $source -or $previous.Destination -ne $destinationFull) {
                throw "Migration record does not match: $migrationRecord"
            }
        }
        New-Item -ItemType Directory -Path $checkoutStorage -Force | Out-Null
        @{ Source = $source; Destination = $destinationFull } | ConvertTo-Json | Set-Content -LiteralPath $migrationRecord
        Write-Host "Moving: $source -> $destinationFull"
        # Robocopy preserves hidden/read-only SDK files and resumes interrupted cross-volume moves.
        & robocopy.exe $source $destinationFull /E /MOVE /SL /SJ /COPY:DAT /DCOPY:DAT /R:1 /W:1 /NP /NFL /NDL /NJH /NJS
        if ($LASTEXITCODE -ge 8) {
            throw "Migration is incomplete; source and destination are preserved. Retry preparation: $source"
        }
        if (Test-Path -LiteralPath $source) {
            if (@(Get-ChildItem -LiteralPath $source -Force).Count -gt 0) {
                throw "Migration left source entries; inspect before retrying: $source"
            }
            Remove-Item -LiteralPath $source -Force
        }
        Remove-Item -LiteralPath $migrationRecord
    }
    else {
        New-Item -ItemType Directory -Path $destinationFull -Force | Out-Null
    }
    New-Item -ItemType Directory -Path (Split-Path -Parent $source) -Force | Out-Null
    New-Item -ItemType Junction -Path $source -Target $destinationFull | Out-Null
    # Flutter requires checkout-relative generated paths; the contents live on D:.
    Ensure-DropboxIgnored $source
    Write-Host "Redirected: $source -> $destinationFull"
}

foreach ($entry in $redirects.GetEnumerator()) {
    Ensure-ExternalDirectory $entry.Key $entry.Value
}

# Keep direct cargo commands and debug staging on the same short external target.
if (Test-Path -LiteralPath $cargoConfig) {
    if ((Get-Content -LiteralPath $cargoConfig -Raw).Replace("`r`n", "`n") -ne $configText) {
        throw "Preserve the existing Cargo configuration and configure external storage explicitly: $cargoConfig"
    }
}
elseif ($CheckOnly) {
    throw "Local Cargo storage configuration is missing: $cargoConfig"
}
else {
    New-Item -ItemType Directory -Path (Split-Path -Parent $cargoConfig) -Force | Out-Null
    [IO.File]::WriteAllText($cargoConfig, $configText, [Text.UTF8Encoding]::new($false))
}

$buildEnvironment = @{
    ALERA_BUILD_STORAGE_ROOT = $StorageRoot
    ALERA_CARGOKIT_TEMP_DIR = $CargoScratchDirectory
    CARGO_TARGET_DIR = $cargoTarget
    CARGO_INCREMENTAL = '0'
    SCCACHE_DIR = (Join-Path $StorageRoot 'sccache')
    PUB_CACHE = (Join-Path $StorageRoot 'pub-cache')
    ZIG_GLOBAL_CACHE_DIR = (Join-Path $StorageRoot 'zig-cache')
    GRADLE_USER_HOME = (Join-Path $StorageRoot 'gradle-cache')
    NPM_CONFIG_CACHE = (Join-Path $StorageRoot 'npm-cache')
    BUN_INSTALL_CACHE_DIR = (Join-Path $StorageRoot 'bun-cache')
    TEMP = (Join-Path $StorageRoot 'temp')
    TMP = (Join-Path $StorageRoot 'temp')
}
foreach ($entry in $buildEnvironment.GetEnumerator()) {
    if (-not $CheckOnly -and $entry.Key -ne 'CARGO_INCREMENTAL') {
        New-Item -ItemType Directory -Path $entry.Value -Force | Out-Null
    }
    Set-Item -LiteralPath "Env:$($entry.Key)" -Value $entry.Value
}
if (-not $CheckOnly) {
    New-Item -ItemType Directory -Path (Split-Path -Parent $storageMetadata) -Force | Out-Null
    $metadataJson = [ordered]@{
        storageRoot = $StorageRoot
        cargoScratchDirectory = $CargoScratchDirectory
        minimumFreeSpaceGB = $MinimumFreeSpaceGB
        environment = $buildEnvironment
    } | ConvertTo-Json -Depth 4
    [IO.File]::WriteAllText($storageMetadata, $metadataJson, [Text.UTF8Encoding]::new($false))
}
Write-Host "Local Windows build storage: $checkoutStorage"
