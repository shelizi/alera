[CmdletBinding()]
param(
    [string]$RemoteHost = 'neo-ai',
    [string]$RemoteDirectory = '/opt/running/fileServer/uploads/alera',
    [string]$ContainerName = 'file-server',
    [ValidateSet('auto', 'rsync', 'scp')][string]$UploadTransport = 'auto',
    [string]$RsyncPath = '',
    [switch]$SkipBuild,
    [switch]$NoPublish,
    [switch]$NoRestart,
    [switch]$AllowNonMain,
    [switch]$UseCurrentWorktree,
    [switch]$KeepWorktree
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Write-Step([string]$Message) {
    Write-Host "`n==> $Message" -ForegroundColor Cyan
}

function Invoke-Checked {
    param(
        [Parameter(Mandatory = $true)][string]$FilePath,
        [Parameter(Mandatory = $false)][string[]]$Arguments = @(),
        [switch]$Capture
    )

    if ($Capture) {
        $output = @(& $FilePath @Arguments 2>&1)
        $exitCode = $LASTEXITCODE
        if ($exitCode -ne 0) {
            $tail = ($output | Select-Object -Last 40) -join "`n"
            throw "Command failed with exit code ${exitCode}: $FilePath $($Arguments -join ' ')`n$tail"
        }
        return @($output | ForEach-Object { [string]$_ })
    }

    & $FilePath @Arguments
    $exitCode = $LASTEXITCODE
    if ($exitCode -ne 0) {
        throw "Command failed with exit code ${exitCode}: $FilePath $($Arguments -join ' ')"
    }
}

function Get-GitText {
    param([Parameter(Mandatory = $true)][string[]]$Arguments)
    $value = @(& git -C $script:RepoRoot @Arguments 2>&1)
    if ($LASTEXITCODE -ne 0) {
        throw "Git command failed: git $($Arguments -join ' ')`n$($value -join "`n")"
    }
    return ($value -join "`n").Trim()
}

function Resolve-RsyncExecutable {
    param([string]$ExplicitPath = '')

    if (-not [string]::IsNullOrWhiteSpace($ExplicitPath)) {
        if (Test-Path -LiteralPath $ExplicitPath -PathType Leaf) {
            return (Resolve-Path -LiteralPath $ExplicitPath).Path
        }
        $explicitCommand = Get-Command $ExplicitPath -ErrorAction SilentlyContinue
        if ($null -ne $explicitCommand) {
            return $explicitCommand.Source
        }
        throw "Configured rsync executable was not found: $ExplicitPath"
    }

    foreach ($commandName in @('rsync.exe', 'rsync')) {
        $command = Get-Command $commandName -ErrorAction SilentlyContinue
        if ($null -ne $command) {
            return $command.Source
        }
    }

    $candidates = @(
        (Join-Path $env:LOCALAPPDATA 'Alera\tools\rsync\runtime\bin\rsync.exe'),
        (Join-Path $env:LOCALAPPDATA 'Alera\tools\rsync\bin\rsync.exe'),
        (Join-Path $env:USERPROFILE '.alera\tools\rsync\bin\rsync.exe'),
        'C:\ProgramData\chocolatey\bin\rsync.exe',
        'C:\Program Files\cwRsync\bin\rsync.exe',
        'C:\msys64\usr\bin\rsync.exe',
        'C:\cygwin64\bin\rsync.exe'
    )
    foreach ($candidate in $candidates) {
        if (Test-Path -LiteralPath $candidate -PathType Leaf) {
            return (Resolve-Path -LiteralPath $candidate).Path
        }
    }

    return $null
}

function Convert-ToRsyncLocalPath {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$RsyncExecutable
    )

    $fullPath = [System.IO.Path]::GetFullPath($Path)
    $cygpath = Join-Path (Split-Path -Parent $RsyncExecutable) 'cygpath.exe'
    if (Test-Path -LiteralPath $cygpath -PathType Leaf) {
        $converted = @(& $cygpath -u $fullPath 2>&1)
        if ($LASTEXITCODE -eq 0 -and $converted.Count -gt 0) {
            return ([string]$converted[0]).Trim()
        }
    }

    if ($fullPath -match '^([A-Za-z]):\\(.*)$') {
        $drive = $matches[1].ToLowerInvariant()
        $rest = $matches[2].Replace('\', '/')
        return "/cygdrive/$drive/$rest"
    }

    return $fullPath.Replace('\', '/')
}

function Sync-RemoteStaging {
    param(
        [Parameter(Mandatory = $true)][string]$LocalPath,
        [Parameter(Mandatory = $true)][string]$RemoteStable,
        [Parameter(Mandatory = $true)][string]$RemoteStaging
    )

    if ($script:UploadTransport -eq 'rsync') {
        Write-Host "Upload transport: rsync ($($script:Rsync))"
        Invoke-Checked -FilePath $script:Ssh -Arguments @(
            $RemoteHost,
            "mkdir -p $RemoteDirectory && if [ ! -f $RemoteStaging ] && [ -f $RemoteStable ]; then cp -f --reflink=auto $RemoteStable $RemoteStaging 2>/dev/null || cp -f $RemoteStable $RemoteStaging; fi"
        )

        $rsyncLocalPath = Convert-ToRsyncLocalPath -Path $LocalPath -RsyncExecutable $script:Rsync
        $bundledSsh = Join-Path (Split-Path -Parent $script:Rsync) 'ssh.exe'
        $sshForRsync = if (Test-Path -LiteralPath $bundledSsh -PathType Leaf) { $bundledSsh } else { $script:Ssh }
        $rsyncSshPath = Convert-ToRsyncLocalPath -Path $sshForRsync -RsyncExecutable $script:Rsync
        $rsyncSshCommand = if ($rsyncSshPath -match '\s') { '"' + $rsyncSshPath + '"' } else { $rsyncSshPath }
        Invoke-Checked -FilePath $script:Rsync -Arguments @(
            '--partial',
            '--inplace',
            '--no-whole-file',
            '--chmod=F644',
            '--stats',
            '--rsync-path=/usr/bin/rsync',
            '-e', $rsyncSshCommand,
            $rsyncLocalPath,
            "${RemoteHost}:$RemoteStaging"
        )
        return
    }

    Write-Host 'Upload transport: scp (rsync unavailable or explicitly disabled)'
    Invoke-Checked -FilePath $script:Ssh -Arguments @($RemoteHost, "mkdir -p $RemoteDirectory")
    Invoke-Checked -FilePath $script:Scp -Arguments @($LocalPath, "${RemoteHost}:$RemoteStaging")
}

function Resolve-GitCommonDirectory {
    $common = Get-GitText @('rev-parse', '--git-common-dir')
    if ([System.IO.Path]::IsPathRooted($common)) {
        return [System.IO.Path]::GetFullPath($common)
    }
    return [System.IO.Path]::GetFullPath((Join-Path $script:RepoRoot $common))
}

function Restore-PinnedSubmodule {
    param(
        [Parameter(Mandatory = $true)][string]$RelativePath,
        [Parameter(Mandatory = $true)][string]$Commit
    )

    $tree = Get-GitText @('ls-tree', $Commit, '--', $RelativePath)
    $match = [regex]::Match($tree, '^160000\s+commit\s+([0-9a-f]{40})\s+')
    if (-not $match.Success) {
        throw "Could not resolve submodule commit for $RelativePath at $Commit."
    }
    $submoduleCommit = $match.Groups[1].Value
    $destination = Join-Path $script:BuildRoot ($RelativePath.Replace('/', '\'))
    New-Item -ItemType Directory -Path $destination -Force | Out-Null

    $commonGit = Resolve-GitCommonDirectory
    $moduleStore = Join-Path $commonGit (Join-Path 'modules' ($RelativePath.Replace('/', '\')))
    $restoredFromLocalStore = $false
    if (Test-Path -LiteralPath $moduleStore) {
        & git "--git-dir=$moduleStore" cat-file -e "${submoduleCommit}^{commit}" 2>$null
        if ($LASTEXITCODE -eq 0) {
            Invoke-Checked -FilePath 'git' -Arguments @(
                "--git-dir=$moduleStore",
                "--work-tree=$destination",
                'checkout', '-f', $submoduleCommit, '--', '.'
            )
            $restoredFromLocalStore = $true
        }
    }

    if (-not $restoredFromLocalStore) {
        Write-Host "Local module store did not contain $RelativePath@$submoduleCommit; falling back to git submodule update."
        Invoke-Checked -FilePath 'git' -Arguments @(
            '-C', $script:BuildRoot,
            'submodule', 'update', '--init', '--recursive', '--', $RelativePath
        )
    }

    $pubspec = Join-Path $destination 'pubspec.yaml'
    if (-not (Test-Path -LiteralPath $pubspec -PathType Leaf)) {
        throw "Submodule restore did not produce $pubspec"
    }
    Write-Host "Restored $RelativePath @ $submoduleCommit"
}

function Assert-BuildRelevantTreeClean {
    $statusText = Get-GitText @('status', '--porcelain=v1', '--untracked-files=no')
    $lines = @($statusText -split "`r?`n") |
        Where-Object { -not [string]::IsNullOrWhiteSpace($_) }

    $blocking = @()
    foreach ($line in $lines) {
        if ($line.Length -lt 4) { continue }
        $path = $line.Substring(3).Replace('\', '/')
        if ($path -like 'skills/alera-remote-release/*') { continue }
        if ($path -like 'docs/history-session/*') { continue }
        $blocking += $line
    }

    if ($blocking.Count -gt 0) {
        throw "Tracked source changes are present. Commit them before publishing:`n$($blocking -join "`n")"
    }
}

function Assert-ReleaseArtifact {
    param([Parameter(Mandatory = $true)][string]$ExpectedCommit)

    $projectPath = Join-Path $script:BuildRoot 'build\windows\x64\runner\Alera.vcxproj'
    $appSoPath = Join-Path $script:BuildRoot 'build\windows\x64\runner\Release\data\app.so'
    $manifestPath = Join-Path $script:BuildRoot 'build\final-link-kit\Alera-Final-Link-Kit\kit-manifest.json'
    $zipPath = Join-Path $script:BuildRoot 'build\final-link-kit\Alera-Final-Link-Kit.zip'

    foreach ($path in @($projectPath, $appSoPath, $manifestPath, $zipPath)) {
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            throw "Required Release artifact is missing: $path"
        }
    }

    $project = Get-Content -LiteralPath $projectPath -Raw
    if ($project -notmatch '<TargetName[^>]*>Alera</TargetName>') {
        throw 'Release project does not target Alera.'
    }
    foreach ($token in @('NDEBUG', 'ALERA_APP_NAME=L&quot;Alera&quot;', 'ALERA_APP_ID=L&quot;dev.leynier.alera&quot;')) {
        $literalToken = $token.Replace('&quot;', '"')
        if ($project -notmatch [regex]::Escape($token) -and $project -notmatch [regex]::Escape($literalToken)) {
            throw "Release project is missing required definition: $literalToken"
        }
    }

    $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
    if ($manifest.product -ne 'Alera') { throw "Unexpected product: $($manifest.product)" }
    if ($manifest.sourceCommit -ne $ExpectedCommit) {
        throw "Manifest sourceCommit mismatch. Expected $ExpectedCommit, got $($manifest.sourceCommit)."
    }
    if ($manifest.outputExecutable -ne 'Alera.exe') {
        throw "Unexpected outputExecutable: $($manifest.outputExecutable)"
    }
    $definitions = @($manifest.compilerDefinitions)
    foreach ($definition in @('NDEBUG', 'ALERA_APP_NAME=L"Alera"', 'ALERA_APP_ID=L"dev.leynier.alera"')) {
        if ($definitions -notcontains $definition) {
            throw "Manifest is missing Release definition: $definition"
        }
    }

    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archive = [System.IO.Compression.ZipFile]::OpenRead($zipPath)
    try {
        $entries = @($archive.Entries | ForEach-Object { $_.FullName.Replace('\', '/') })
        $appSoEntries = @($entries | Where-Object { $_ -eq 'payload/data/app.so' })
        if ($appSoEntries.Count -ne 1) {
            $appSoCandidates = @($entries | Where-Object { $_ -match '(^|/)app\.so$' })
            throw "Final-Link ZIP must contain exactly one payload/data/app.so. Found $($appSoEntries.Count). app.so candidates: $($appSoCandidates -join ', ')"
        }
        if ($entries -notcontains 'payload/resources/alera/alera.exe') {
            throw 'Final-Link ZIP is missing payload/resources/alera/alera.exe.'
        }
        if (@($entries | Where-Object { $_ -eq 'Alera.exe' -or $_ -eq 'payload/Alera.exe' }).Count -gt 0) {
            throw 'Final-Link ZIP unexpectedly contains the build-machine Alera.exe.'
        }
    }
    finally {
        $archive.Dispose()
    }

    $file = Get-Item -LiteralPath $zipPath
    $hash = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToUpperInvariant()
    return [pscustomobject]@{
        Commit = $ExpectedCommit
        Version = [string]$manifest.version
        ZipPath = $zipPath
        Size = [int64]$file.Length
        Sha256 = $hash
    }
}

function Get-RemoteHash {
    param([Parameter(Mandatory = $true)][string]$RemotePath)
    $lines = Invoke-Checked -FilePath $script:Ssh -Arguments @(
        $RemoteHost,
        "sha256sum $RemotePath && unzip -t $RemotePath >/dev/null && echo ZIP_OK"
    ) -Capture
    $text = $lines -join "`n"
    $match = [regex]::Match($text, '(?im)^([0-9a-f]{64})\s+')
    if (-not $match.Success) {
        throw "Could not parse remote SHA-256 for $RemotePath.`n$text"
    }
    if ($text -notmatch '(?m)^ZIP_OK$') {
        throw "Remote ZIP validation did not return ZIP_OK for $RemotePath.`n$text"
    }
    return $match.Groups[1].Value.ToUpperInvariant()
}

if (-not [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform(
    [System.Runtime.InteropServices.OSPlatform]::Windows
)) {
    throw 'This release publisher only supports Windows.'
}

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$script:RepoRoot = (Resolve-Path -LiteralPath (Join-Path $scriptDir '..\..')).Path
$script:Ssh = (Get-Command ssh.exe -ErrorAction Stop).Source
$script:Scp = (Get-Command scp.exe -ErrorAction Stop).Source
$script:Rsync = Resolve-RsyncExecutable -ExplicitPath $RsyncPath
$script:UploadTransport = $UploadTransport
if ($UploadTransport -eq 'rsync' -and $null -eq $script:Rsync) {
    throw 'UploadTransport=rsync was requested, but rsync could not be found. Install rsync or pass -RsyncPath.'
}
if ($UploadTransport -eq 'auto') {
    $script:UploadTransport = if ($null -ne $script:Rsync) { 'rsync' } else { 'scp' }
}
if ($script:UploadTransport -eq 'rsync') {
    $remoteRsync = Invoke-Checked -FilePath $script:Ssh -Arguments @(
        $RemoteHost,
        "command -v rsync >/dev/null && rsync --version | head -1"
    ) -Capture
    Write-Host "Remote rsync: $($remoteRsync[0])"
}

$lockPath = Join-Path $script:RepoRoot 'build\windows-final-link-publish.lock'
New-Item -ItemType Directory -Force -Path (Split-Path -Parent $lockPath) | Out-Null
$lock = $null
$releaseWorktree = $null
$createdReleaseWorktree = $false
$releaseCompleted = $false
try {
    $lock = [System.IO.File]::Open($lockPath, [System.IO.FileMode]::OpenOrCreate, [System.IO.FileAccess]::ReadWrite, [System.IO.FileShare]::None)
}
catch {
    throw "Another Alera Final-Link publish is already running: $lockPath"
}

try {
    Write-Step 'Preflight'
    $branch = Get-GitText @('branch', '--show-current')
    $commit = if ($AllowNonMain) { Get-GitText @('rev-parse', 'HEAD') } else { Get-GitText @('rev-parse', 'main') }
    if (-not $AllowNonMain -and $branch -ne 'main') {
        throw "Expected branch main, found $branch. Use -AllowNonMain only for an intentional test release."
    }
    Write-Host "Branch: $branch"
    Write-Host "Commit: $commit"

    if ($UseCurrentWorktree) {
        if (-not $SkipBuild) {
            Assert-BuildRelevantTreeClean
        }
        $script:BuildRoot = $script:RepoRoot
        Write-Host "Build root: $($script:BuildRoot) (current worktree)"
    }
    else {
        if ($SkipBuild) {
            throw '-SkipBuild requires -UseCurrentWorktree because a fresh pinned worktree has no existing build artifacts.'
        }
        $shortCommit = $commit.Substring(0, 8)
        $releaseWorktree = Join-Path $script:RepoRoot ".worktrees\release-auto-$shortCommit-$PID"
        Write-Step 'Create isolated pinned release worktree'
        Invoke-Checked -FilePath 'git' -Arguments @('-C', $script:RepoRoot, 'worktree', 'add', '--detach', $releaseWorktree, $commit)
        $createdReleaseWorktree = $true
        $script:BuildRoot = (Resolve-Path -LiteralPath $releaseWorktree).Path
        Restore-PinnedSubmodule -RelativePath 'third_party/xterm' -Commit $commit
        Restore-PinnedSubmodule -RelativePath 'third_party/dart_terminal' -Commit $commit
        Write-Host "Build root: $($script:BuildRoot)"
    }

    $exporter = Join-Path $script:BuildRoot 'tool\windows-finalizer\export_final_link_kit.ps1'
    if (-not (Test-Path -LiteralPath $exporter -PathType Leaf)) {
        throw "Final-Link exporter was not found in the pinned source tree: $exporter"
    }

    Write-Step 'Build and export the authoritative Final-Link Kit'
    $arguments = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $exporter, '-Force')
    if ($SkipBuild) { $arguments += '-SkipBuild' }
    Push-Location $script:BuildRoot
    try {
        Invoke-Checked -FilePath 'powershell.exe' -Arguments $arguments
    }
    finally {
        Pop-Location
    }

    Write-Step 'Verify Release identity and package contents'
    $artifact = Assert-ReleaseArtifact -ExpectedCommit $commit
    Write-Host "Version: $($artifact.Version)"
    Write-Host "Size:    $($artifact.Size) bytes"
    Write-Host "SHA256:  $($artifact.Sha256)"

    if ($NoPublish) {
        Write-Host 'Build/verification completed. Remote publication skipped by -NoPublish.' -ForegroundColor Yellow
        $releaseCompleted = $true
        return
    }

    $remoteStable = "$RemoteDirectory/Alera-Final-Link-Kit.zip"
    $remoteStaging = "$RemoteDirectory/.Alera-Final-Link-Kit.uploading.zip"

    Write-Step 'Sync to remote staging path'
    Sync-RemoteStaging -LocalPath $artifact.ZipPath -RemoteStable $remoteStable -RemoteStaging $remoteStaging
    $stagingHash = Get-RemoteHash -RemotePath $remoteStaging
    if ($stagingHash -ne $artifact.Sha256) {
        throw "Remote staging hash mismatch. Local $($artifact.Sha256), remote $stagingHash."
    }

    Write-Step 'Atomically promote and verify remote stable package'
    Invoke-Checked -FilePath $script:Ssh -Arguments @($RemoteHost, "mv -f $remoteStaging $remoteStable")
    $stableHash = Get-RemoteHash -RemotePath $remoteStable
    if ($stableHash -ne $artifact.Sha256) {
        throw "Remote stable hash mismatch. Local $($artifact.Sha256), remote $stableHash."
    }

    if (-not $NoRestart) {
        Write-Step "Restart $ContainerName and verify the served package"
        $containerPath = '/app/uploads/alera/Alera-Final-Link-Kit.zip'
        Invoke-Checked -FilePath $script:Ssh -Arguments @($RemoteHost, "docker restart $ContainerName >/dev/null")

        $containerHash = $null
        $lastContainerOutput = ''
        for ($attempt = 1; $attempt -le 10; $attempt++) {
            try {
                $remote = Invoke-Checked -FilePath $script:Ssh -Arguments @(
                    $RemoteHost,
                    "docker exec $ContainerName sha256sum $containerPath && docker ps --filter name=^/$ContainerName`$ --format '{{.Names}}\t{{.Status}}\t{{.Image}}'"
                ) -Capture
                $lastContainerOutput = $remote -join "`n"
                $containerHashMatch = [regex]::Match(
                    $lastContainerOutput,
                    '(?im)^([0-9a-f]{64})\s+/app/uploads/alera/Alera-Final-Link-Kit\.zip\s*$'
                )
                if ($containerHashMatch.Success) {
                    $containerHash = $containerHashMatch.Groups[1].Value.ToUpperInvariant()
                    break
                }
            }
            catch {
                $lastContainerOutput = $_.Exception.Message
            }

            if ($attempt -lt 10) {
                Start-Sleep -Seconds 1
            }
        }

        if (-not $containerHash) {
            throw "Could not verify the package inside $ContainerName after restart.`n$lastContainerOutput"
        }
        if ($containerHash -ne $artifact.Sha256) {
            throw "Container-visible hash mismatch. Local $($artifact.Sha256), container $containerHash."
        }
    }

    Write-Step 'Completed'
    Write-Host "Commit:  $commit"
    Write-Host "Package: $($artifact.ZipPath)"
    Write-Host "Remote:  $remoteStable"
    Write-Host "SHA256:  $($artifact.Sha256)"
    Write-Host 'Release publish completed successfully.' -ForegroundColor Green
    $releaseCompleted = $true
}
finally {
    if ($null -ne $lock) { $lock.Dispose() }
    Remove-Item -LiteralPath $lockPath -Force -ErrorAction SilentlyContinue
    if ($createdReleaseWorktree -and -not $KeepWorktree -and $releaseCompleted -and $releaseWorktree) {
        try {
            & git -C $script:RepoRoot worktree remove --force $releaseWorktree 2>$null | Out-Null
        }
        catch {
            Write-Warning "Could not remove temporary release worktree: $releaseWorktree"
        }
    }
    elseif ($createdReleaseWorktree -and -not $releaseCompleted -and $releaseWorktree) {
        Write-Warning "Release failed; preserving diagnostic worktree: $releaseWorktree"
    }
}
