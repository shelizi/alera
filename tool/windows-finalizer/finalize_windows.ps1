param(
    [string]$OutputDirectory,
    [string]$PlatformToolset,
    [string]$WindowsSdkVersion,
    [string]$WindowsSdkRoot,
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
    param(
        [Parameter(Mandatory = $true)][string]$VsWhere,
        [string]$RequiredToolset
    )

    $installPaths = @(
        & $VsWhere -all -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2>$null |
            Where-Object { $_ } |
            ForEach-Object { $_.Trim() }
    )
    if ($installPaths.Count -eq 0) {
        throw 'No Visual Studio/Build Tools installation with the x64 C++ toolchain was found.'
    }

    foreach ($installPath in $installPaths) {
        if ($RequiredToolset) {
            $vcMsBuildRoot = Join-Path $installPath 'MSBuild\Microsoft\VC'
            $toolsetFound = $false
            if (Test-Path -LiteralPath $vcMsBuildRoot) {
                $toolsetFound = $null -ne (Get-ChildItem -LiteralPath $vcMsBuildRoot -Recurse -Directory -ErrorAction SilentlyContinue |
                    Where-Object { $_.Parent.Name -eq 'PlatformToolsets' -and $_.Name -eq $RequiredToolset } |
                    Select-Object -First 1)
            }
            if (-not $toolsetFound) {
                continue
            }
        }

        foreach ($candidate in @(
            (Join-Path $installPath 'MSBuild\Current\Bin\MSBuild.exe'),
            (Join-Path $installPath 'MSBuild\17.0\Bin\MSBuild.exe')
        )) {
            if (Test-Path -LiteralPath $candidate) {
                return [pscustomobject]@{ MSBuild = $candidate; InstallPath = $installPath }
            }
        }
    }

    if ($RequiredToolset) {
        throw "No Visual Studio/Build Tools installation with platform toolset $RequiredToolset was found. For v143, install Visual Studio 2022 Build Tools with Desktop development with C++."
    }
    throw 'MSBuild.exe was not found in the installed Visual Studio C++ instances.'
}

function Resolve-WindowsSdk {
    param(
        [string]$Root,
        [string]$Requested
    )

    if ($Root) {
        $sdkRoot = [System.IO.Path]::GetFullPath($Root).TrimEnd([char]'\')
    }
    else {
        $sdkRoot = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10'
    }

    $includeRoot = Join-Path $sdkRoot 'Include'
    if (-not (Test-Path -LiteralPath $includeRoot -PathType Container)) {
        if ($Root) {
            throw "Portable Windows SDK Include directory was not found under: $sdkRoot"
        }
        throw 'Windows 10/11 SDK headers were not found. Supply -WindowsSdkRoot with a portable Windows Kits\10 directory.'
    }

    $available = @(
        Get-ChildItem -LiteralPath $includeRoot -Directory |
            Where-Object { $_.Name -match '^10\.0\.\d+\.\d+$' } |
            Sort-Object { [version]$_.Name } -Descending
    )
    if ($available.Count -eq 0) {
        throw "No usable Windows SDK version was found under: $includeRoot"
    }

    $version = $null
    if ($Requested) {
        $requestedNormalized = $Requested.Trim().TrimEnd([char]'\')
        $requestedPath = Join-Path $includeRoot $requestedNormalized
        if (Test-Path -LiteralPath $requestedPath -PathType Container) {
            $version = $requestedNormalized
        }
        else {
            Write-Warning "Requested Windows SDK $requestedNormalized is unavailable under $sdkRoot; using $($available[0].Name)."
        }
    }
    if (-not $version) {
        $version = $available[0].Name
    }

    $required = @(
        "Include\$version\um\Windows.h",
        "Include\$version\ucrt\stdio.h",
        "Include\$version\shared\sdkddkver.h",
        "Lib\$version\um\x64\kernel32.lib",
        "Lib\$version\ucrt\x64\ucrt.lib",
        "bin\$version\x64\rc.exe",
        "bin\$version\x64\mt.exe"
    )
    foreach ($relative in $required) {
        $candidate = Join-Path $sdkRoot $relative
        if (-not (Test-Path -LiteralPath $candidate -PathType Leaf)) {
            throw "Windows SDK is incomplete; required file is missing: $candidate"
        }
    }

    return [pscustomobject]@{
        Root = $sdkRoot
        Version = $version
        IsPortable = [bool](-not [string]::IsNullOrWhiteSpace($Root))
    }
}

$kitRoot = (Resolve-Path -LiteralPath $PSScriptRoot).Path
$manifestPath = Join-Path $kitRoot 'kit-manifest.json'
$projectPath = Join-Path $kitRoot 'AleraFinal.vcxproj'
$payloadPath = Join-Path $kitRoot 'payload'
if (-not (Test-Path -LiteralPath $manifestPath)) { throw "Missing kit manifest: $manifestPath" }
if (-not (Test-Path -LiteralPath $projectPath)) { throw "Missing standalone MSBuild project: $projectPath" }
if (-not (Test-Path -LiteralPath $payloadPath -PathType Container)) { throw "Missing runtime payload: $payloadPath" }

$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
$schemaVersion = [int]$manifest.schemaVersion
if ($schemaVersion -lt 1 -or $schemaVersion -gt 2) {
    throw "Unsupported Final-Link Kit schema: $($manifest.schemaVersion)"
}

if (-not $SkipHashVerification) {
    Write-Step 'Verifying Final-Link Kit transfer hashes'
    Verify-KitHashes -KitRoot $kitRoot
}

$manifestToolset = if (
    $schemaVersion -ge 2 -and
    ($manifest.PSObject.Properties.Name -contains 'targetPlatformToolset') -and
    [string]$manifest.targetPlatformToolset
) {
    [string]$manifest.targetPlatformToolset
}
else {
    [string]$manifest.sourcePlatformToolset
}
$effectiveToolset = if ($PlatformToolset) { $PlatformToolset } else { $manifestToolset }
$requestedSdk = if ($WindowsSdkVersion) { $WindowsSdkVersion } else { [string]$manifest.sourceWindowsSdkVersion }
$sdk = Resolve-WindowsSdk -Root $WindowsSdkRoot -Requested $requestedSdk
$effectiveSdk = [string]$sdk.Version

Write-Step 'Locating the installed Visual Studio C++ toolchain'
$vswhere = Find-VsWhere
$vs = Find-MSBuild -VsWhere $vswhere -RequiredToolset $effectiveToolset

Write-Host "Visual Studio: $($vs.InstallPath)"
Write-Host "MSBuild:       $($vs.MSBuild)"
Write-Host "Toolset:       $effectiveToolset"
Write-Host "Windows SDK:   $effectiveSdk"
Write-Host "SDK root:      $($sdk.Root)"
if ($sdk.IsPortable) { Write-Host 'SDK mode:      portable directory (no SDK installation required)' } else { Write-Host 'SDK mode:      installed Windows SDK' }
Write-Host "Source commit: $($manifest.sourceCommit)"

if ($VerifyOnly) {
    Write-Host 'Final-Link Kit, installed MSVC toolset, and selected Windows SDK root are ready.' -ForegroundColor Green
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

Write-Step 'Compiling the local Flutter C++ wrapper and Windows runner, then linking Alera'
$outDirForMsbuild = (([string]$OutputDirectory).TrimEnd([char]'\', [char]'/') -replace '\\', '/') + '/'
$intDirForMsbuild = (((Join-Path $buildRoot 'obj').TrimEnd([char]'\', [char]'/')) -replace '\\', '/') + '/'
$msbuildArgs = @(
    $projectPath,
    '/nologo',
    '/m',
    '/t:Rebuild',
    '/p:Configuration=Release',
    '/p:Platform=x64',
    "/p:OutDir=$outDirForMsbuild",
    "/p:IntDir=$intDirForMsbuild",
    "/p:WindowsTargetPlatformVersion=$effectiveSdk",
    '/v:minimal'
)
if ($effectiveToolset) {
    $msbuildArgs += "/p:PlatformToolset=$effectiveToolset"
}

$sdkRootForMsbuild = (([string]$sdk.Root).TrimEnd([char]'\') -replace '\\', '/') + '/'
$msbuildArgs += @(
    "/p:WindowsSdkDir=$sdkRootForMsbuild",
    "/p:WindowsSdkDir_10=$sdkRootForMsbuild",
    "/p:UniversalCRTSdkDir=$sdkRootForMsbuild",
    "/p:UniversalCRTSdkDir_10=$sdkRootForMsbuild",
    "/p:TargetPlatformVersion=$effectiveSdk",
    "/p:TargetUniversalCRTVersion=$effectiveSdk",
    "/p:UCRTVersion=$effectiveSdk",
    "/p:WindowsSdkVerBinPath=$($sdkRootForMsbuild)bin/$effectiveSdk/"
)

Write-Host "SDK bin:       $($sdk.Root)\bin\$effectiveSdk\x64"
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
Write-Host 'No Flutter CLI, Dart, Cargo, Rust, Zig, CMake, Ninja, Windows SDK installer, or signing step was invoked on this target machine.'
