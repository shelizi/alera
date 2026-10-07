[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$prepare = Join-Path $PSScriptRoot 'prepare_windows_build_storage.ps1'
$testDrive = @(
    [IO.DriveInfo]::GetDrives() |
        Where-Object { $_.IsReady -and $_.DriveType -eq [IO.DriveType]::Fixed } |
        Sort-Object AvailableFreeSpace -Descending
)[0]
if ($null -eq $testDrive) { throw 'No ready local fixed drive is available for build-storage tests' }
$testRoot = Join-Path $testDrive.RootDirectory.FullName "alera-build-storage-tests\$([guid]::NewGuid().ToString('N'))"
$repo = Join-Path $testRoot 'source'
$storage = Join-Path $testRoot 'outputs'
$scratch = Join-Path $testRoot 'c'
New-Item -ItemType Directory -Path (Join-Path $repo 'build') -Force | Out-Null
Set-Content -LiteralPath (Join-Path $repo 'pubspec.yaml') -Value 'name: alera_storage_test'
Set-Content -LiteralPath (Join-Path $repo 'build\retained.txt') -Value 'preserve existing output'
& $prepare -RepoRoot $repo -StorageRoot $storage -CargoScratchDirectory $scratch -MinimumFreeSpaceGB 0
$build = Get-Item -LiteralPath (Join-Path $repo 'build') -Force
if ($build.LinkType -ne 'Junction') { throw 'Build output was not redirected' }
if ((Get-Content -LiteralPath (Join-Path ([string]$build.Target) 'retained.txt')).Trim() -ne 'preserve existing output') {
    throw 'Migration lost existing output'
}
Set-Content -LiteralPath (Join-Path $repo 'build\new-output.txt') -Value 'written through checkout'
if (-not (Test-Path -LiteralPath (Join-Path ([string]$build.Target) 'new-output.txt'))) {
    throw 'Checkout writes did not reach external storage'
}
& $prepare -RepoRoot $repo -StorageRoot $storage -CargoScratchDirectory $scratch -MinimumFreeSpaceGB 0
& $prepare -RepoRoot $repo -StorageRoot $storage -CargoScratchDirectory $scratch -MinimumFreeSpaceGB 0 -CheckOnly
Write-Host 'PASS: migration preserves output, writes land on external root storage, and preparation is idempotent'

$metadata = Get-Content -LiteralPath (Join-Path $repo '.cargo\alera-build-storage.json') -Raw | ConvertFrom-Json
if ($metadata.storageRoot -ne $storage -or $metadata.cargoScratchDirectory -ne $scratch) {
    throw 'Prepared storage metadata did not preserve the selected roots'
}
if ($metadata.environment.ALERA_BUILD_STORAGE_ROOT -ne $storage -or
    $metadata.environment.ALERA_CARGOKIT_TEMP_DIR -ne $scratch) {
    throw 'Prepared storage metadata did not expose the selected environment'
}
Write-Host 'PASS: selected build roots are persisted for Dart/debug tooling'

Set-Content -LiteralPath $build.FullName -Stream com.dropbox.ignored -Value 0
$failed = $false
try { & $prepare -RepoRoot $repo -StorageRoot $storage -CargoScratchDirectory $scratch -MinimumFreeSpaceGB 0 -CheckOnly }
catch {
    if ($_.Exception.Message -notlike 'Dropbox ignore attribute is missing:*') { throw }
    $failed = $true
}
if (-not $failed) { throw 'Accepted a generated directory without Dropbox ignore' }
& $prepare -RepoRoot $repo -StorageRoot $storage -CargoScratchDirectory $scratch -MinimumFreeSpaceGB 0
if ([string](Get-Content -LiteralPath $build.FullName -Stream com.dropbox.ignored) -ne '1') {
    throw 'Did not repair the Dropbox ignore attribute on an existing junction'
}
& $prepare -RepoRoot $repo -StorageRoot $storage -CargoScratchDirectory $scratch -MinimumFreeSpaceGB 0 -CheckOnly
Write-Host 'PASS: missing Dropbox ignore is detected and repaired on existing junctions'

$failed = $false
$dropboxStorage = Join-Path $testDrive.RootDirectory.FullName 'Dropbox\alera-storage-test'
try { & $prepare -RepoRoot $repo -StorageRoot $dropboxStorage -CargoScratchDirectory $scratch -MinimumFreeSpaceGB 0 }
catch { $failed = $true }
if (-not $failed) { throw 'Accepted build storage inside a Dropbox path' }
Write-Host 'PASS: Dropbox paths are rejected regardless of drive letter'

$oldStorageRoot = $env:ALERA_BUILD_STORAGE_ROOT
$oldScratchRoot = $env:ALERA_CARGOKIT_TEMP_DIR
try {
    $env:ALERA_BUILD_STORAGE_ROOT = $storage
    $env:ALERA_CARGOKIT_TEMP_DIR = $scratch
    & $prepare -RepoRoot $repo -MinimumFreeSpaceGB 0 -CheckOnly
}
finally {
    if ($null -eq $oldStorageRoot) { Remove-Item Env:ALERA_BUILD_STORAGE_ROOT -ErrorAction SilentlyContinue }
    else { $env:ALERA_BUILD_STORAGE_ROOT = $oldStorageRoot }
    if ($null -eq $oldScratchRoot) { Remove-Item Env:ALERA_CARGOKIT_TEMP_DIR -ErrorAction SilentlyContinue }
    else { $env:ALERA_CARGOKIT_TEMP_DIR = $oldScratchRoot }
}
Write-Host 'PASS: environment overrides select any fixed-drive root without a D: dependency'

$lockedRepo = Join-Path $testRoot 'locked-source'
New-Item -ItemType Directory -Path (Join-Path $lockedRepo 'build') -Force | Out-Null
Set-Content -LiteralPath (Join-Path $lockedRepo 'pubspec.yaml') -Value 'name: alera_storage_test'
$stream = [IO.File]::Open((Join-Path $lockedRepo 'build\build.lock'), [IO.FileMode]::Create, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
try {
    $failed = $false
    try { & $prepare -RepoRoot $lockedRepo -StorageRoot $storage -CargoScratchDirectory $scratch -MinimumFreeSpaceGB 0 }
    catch { $failed = $true }
    if (-not $failed) { throw 'Migrated a locked build directory' }
    if ((Get-Item -LiteralPath (Join-Path $lockedRepo 'build')).LinkType) { throw 'Changed a locked directory' }
}
finally { $stream.Dispose() }
Write-Host 'PASS: an active build lock prevents migration'

$configRepo = Join-Path $testRoot 'configured-source'
New-Item -ItemType Directory -Path (Join-Path $configRepo '.cargo') -Force | Out-Null
Set-Content -LiteralPath (Join-Path $configRepo 'pubspec.yaml') -Value 'name: alera_storage_test'
$config = Join-Path $configRepo '.cargo\config.toml'
Set-Content -LiteralPath $config -Value '[net]'
$failed = $false
try { & $prepare -RepoRoot $configRepo -StorageRoot $storage -CargoScratchDirectory $scratch -MinimumFreeSpaceGB 0 }
catch { $failed = $true }
if (-not $failed -or (Get-Content -LiteralPath $config).Trim() -ne '[net]') { throw 'Overwrote existing Cargo configuration' }
if (Test-Path -LiteralPath (Join-Path $configRepo 'build')) { throw 'Mutated directories before detecting a Cargo configuration conflict' }
Write-Host 'PASS: existing Cargo configuration is preserved before any migration'
