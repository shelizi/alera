param(
    [string]$OutputDirectory,
    [string]$PlatformToolset,
    [string]$WindowsSdkVersion,
    [switch]$VerifyOnly,
    [switch]$SkipHashVerification,
    [switch]$Force
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Write-Step([string]$Message) {
    Write-Host "`n==> $Message" -ForegroundColor Cyan
}

function Verify-KitHashes {
    param([Parameter(Mandatory = $true)][string]$KitRoot)

    $hashPath = Join-Path $KitRoot 'sha256sums.json'
    if (-not (Test-Path -LiteralPath $hashPath)) {
        throw "Missing transfer hash manifest: $hashPath"
    }

    $entries = Get-Content -LiteralPath $hashPath -Raw | ConvertFrom-Json
    foreach ($entry in $entries) {
        $relative = ([string]$entry.path) -replace '/', '\'
        $path = Join-Path $KitRoot $relative
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            throw "Final-Link Kit file is missing: $relative"
        }
        $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash
        if ($actual -ne [string]$entry.sha256) {
            throw "Final-Link Kit hash mismatch: $relative"
        }
    }
}

function Find-VsWhere {
    $candidates = @(
        @(
            (Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'),
            (Join-Path $env:ProgramFiles 'Microsoft Visual Studio\Installer\vswhere.exe')
        ) | Where-Object { $_ -and (Test-Path -LiteralPath $_) }
    )

    if ($candidates.Count -gt 0) {
        return $candidates[0]
    }
    $command = Get-Command vswhere -ErrorAction SilentlyContinue
    if ($command) { return $command.Source }
    throw 'Visual Studio Installer/vswhere was not found. Install Visual Studio Build Tools with Desktop development with C++.'
}

function Find-MSBuild {
    param([Parameter(Mandatory = $true)][string]$VsWhere)

    $installPath = (& $VsWhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2>$null | Select-Object -First 1)
    if (-not $installPath) {
        throw 'No Visual Studio/Build Tools installation with the x64 C++ toolchain was found.'
    }
    $installPath = $installPath.Trim()

    foreach ($candidate in @(
        (Join-Path $installPath 'MSBuild\Current\Bin\MSBuild.exe'),
        (Join-Path $installPath 'MSBuild\17.0\Bin\MSBuild.exe')
    )) {
        if (Test-Path -LiteralPath $candidate) {
            return [pscustomobject]@{ MSBuild = $candidate; InstallPath = $installPath }
        }
    }
    throw "MSBuild.exe was not found under $installPath"
}

function Resolve-WindowsSdkVersion {
    param([string]$Requested)

    $kitsRoot = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\Include'
    if (-not (Test-Path -LiteralPath $kitsRoot)) {
        throw 'Windows 10/11 SDK headers were not found. Install a Windows SDK through Visual Studio Build Tools.'
    }

    if ($Requested -and (Test-Path -LiteralPath (Join-Path $kitsRoot $Requested))) {
        return $Requested
    }

    $available = @(
        Get-ChildItem -LiteralPath $kitsRoot -Directory |
            Where-Object { $_.Name -match '^10\.0\.\d+\.\d+$' } |
            Sort-Object { [version]$_.Name } -Descending
    )
    if ($available.Count -eq 0) {
        throw 'No usable Windows 10/11 SDK version was found.'
    }
    if ($Requested) {
        Write-Warning "Requested Windows SDK $Requested is unavailable; using $($available[0].Name)."
    }
    return $available[0].Name
}

$kitRoot = (Resolve-Path -LiteralPath $PSScriptRoot).Path
$manifestPath = Join-Path $kitRoot 'kit-manifest.json'
$projectPath = Join-Path $kitRoot 'AleraFinal.vcxproj'
$payloadPath = Join-Path $kitRoot 'payload'
if (-not (Test-Path -LiteralPath $manifestPath)) { throw "Missing kit manifest: $manifestPath" }
if (-not (Test-Path -LiteralPath $projectPath)) { throw "Missing standalone MSBuild project: $projectPath" }
if (-not (Test-Path -LiteralPath $payloadPath -PathType Container)) { throw "Missing runtime payload: $payloadPath" }

$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
if ([int]$manifest.schemaVersion -ne 1) {
    throw "Unsupported Final-Link Kit schema: $($manifest.schemaVersion)"
}

if (-not $SkipHashVerification) {
    Write-Step 'Verifying Final-Link Kit transfer hashes'
    Verify-KitHashes -KitRoot $kitRoot
}

Write-Step 'Locating the local Visual Studio C++ toolchain'
$vswhere = Find-VsWhere
$vs = Find-MSBuild -VsWhere $vswhere
$effectiveToolset = if ($PlatformToolset) { $PlatformToolset } else { [string]$manifest.sourcePlatformToolset }
$effectiveSdk = Resolve-WindowsSdkVersion -Requested $(if ($WindowsSdkVersion) { $WindowsSdkVersion } else { [string]$manifest.sourceWindowsSdkVersion })

Write-Host "Visual Studio: $($vs.InstallPath)"
Write-Host "MSBuild:      $($vs.MSBuild)"
Write-Host "Toolset:      $effectiveToolset"
Write-Host "Windows SDK:  $effectiveSdk"
Write-Host "Source commit:$($manifest.sourceCommit)"

if ($VerifyOnly) {
    Write-Host 'Final-Link Kit and local linker environment are ready.' -ForegroundColor Green
    exit 0
}

if (-not $OutputDirectory) {
    $OutputDirectory = Join-Path $kitRoot 'out\Alera'
}
$OutputDirectory = [System.IO.Path]::GetFullPath($OutputDirectory)

if (Test-Path -LiteralPath $OutputDirectory) {
    $marker = Join-Path $OutputDirectory '.alera-finalizer-output'
    if (-not $Force -and -not (Test-Path -LiteralPath $marker)) {
        throw "Output directory already exists and was not created by this finalizer: $OutputDirectory. Use -Force only if it is safe to replace."
    }
    Remove-Item -LiteralPath $OutputDirectory -Recurse -Force
}
New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
New-Item -ItemType File -Path (Join-Path $OutputDirectory '.alera-finalizer-output') -Force | Out-Null

Write-Step 'Staging the fully prebuilt Flutter/Rust/Ghostty runtime payload'
Get-ChildItem -LiteralPath $payloadPath -Force | ForEach-Object {
    Copy-Item -LiteralPath $_.FullName -Destination $OutputDirectory -Recurse -Force
}

$buildRoot = Join-Path $kitRoot '.finalizer-build'
if (Test-Path -LiteralPath $buildRoot) {
    Remove-Item -LiteralPath $buildRoot -Recurse -Force
}
New-Item -ItemType Directory -Path $buildRoot -Force | Out-Null

Write-Step 'Compiling and linking only the local Windows runner'
$msbuildArgs = @(
    $projectPath,
    '/nologo',
    '/m',
    '/t:Rebuild',
    '/p:Configuration=Release',
    '/p:Platform=x64',
    "/p:OutDir=$OutputDirectory\",
    "/p:IntDir=$buildRoot\obj\",
    "/p:WindowsTargetPlatformVersion=$effectiveSdk",
    '/v:minimal'
)
if ($effectiveToolset) {
    $msbuildArgs += "/p:PlatformToolset=$effectiveToolset"
}

& $vs.MSBuild @msbuildArgs
if ($LASTEXITCODE -ne 0) {
    throw "Final local MSVC link failed with exit code $LASTEXITCODE."
}

$exe = Join-Path $OutputDirectory ([string]$manifest.outputExecutable)
if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) {
    throw "MSBuild completed but the local executable was not produced: $exe"
}

Remove-Item -LiteralPath (Join-Path $OutputDirectory '.alera-finalizer-output') -Force -ErrorAction SilentlyContinue
$hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $exe).Hash
Write-Host "`nLocal executable created successfully." -ForegroundColor Green
Write-Host "EXE:    $exe"
Write-Host "SHA256: $hash"
Write-Host 'No Flutter, Dart, Cargo, Rust, Zig, CMake, Ninja, or signing step was invoked on this target machine.'
