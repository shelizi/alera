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
    [switch]$KeepWorktree,
    [string]$ExpectedCommit = '',
    [string]$ExpectedVersion = '',
    [switch]$ProtectStableVersion,
    [switch]$AllowSamePublishedCommit,
    [switch]$VerifyPublishedOnly,
    [ValidatePattern('^[0-9A-Fa-f]{64}$')][string]$ExpectedSha256,
    [string]$ReceiptPath = ''
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
            $archivePath = Join-Path ([System.IO.Path]::GetTempPath()) ("alera-submodule-" + [guid]::NewGuid().ToString('N') + '.tar')
            try {
                Invoke-Checked -FilePath 'git' -Arguments @(
                    "--git-dir=$moduleStore",
                    'archive', '--format=tar', "--output=$archivePath", $submoduleCommit
                )
                Invoke-Checked -FilePath 'tar.exe' -Arguments @('-xf', $archivePath, '-C', $destination)
                $restoredFromLocalStore = $true
            }
            finally {
                Remove-Item -LiteralPath $archivePath -Force -ErrorAction SilentlyContinue
            }
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
    $changelogPath = Join-Path $script:BuildRoot 'build\final-link-kit\Alera-Final-Link-Kit\CHANGELOG.md'
    $zipPath = Join-Path $script:BuildRoot 'build\final-link-kit\Alera-Final-Link-Kit.zip'

    foreach ($path in @($projectPath, $appSoPath, $manifestPath, $changelogPath, $zipPath)) {
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
    if ($manifest.changelogFile -ne 'CHANGELOG.md') {
        throw 'Release manifest does not identify the Final-Link changelog.'
    }
    $changelog = [System.IO.File]::ReadAllText($changelogPath)
    $tick = [char]96
    if (-not $changelog.Contains("- **Version:** $tick$($manifest.version)$tick")) {
        throw 'Final-Link changelog version does not match the Release manifest.'
    }
    if (-not $changelog.Contains("- **Source commit:** $tick$ExpectedCommit$tick")) {
        throw 'Final-Link changelog source commit does not match the Release manifest.'
    }
    $hashFile = Join-Path $script:BuildRoot 'build\final-link-kit\Alera-Final-Link-Kit\sha256sums.json'
    $hashEntries = Get-Content -LiteralPath $hashFile -Raw | ConvertFrom-Json
    $changelogHash = @($hashEntries | Where-Object { $_.path -eq 'CHANGELOG.md' })
    if ($changelogHash.Count -ne 1 -or $changelogHash[0].sha256 -ne (Get-FileHash -LiteralPath $changelogPath -Algorithm SHA256).Hash) {
        throw 'Final-Link changelog is missing from sha256sums.json or has the wrong hash.'
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
        $zipChangelogEntry = $archive.GetEntry('CHANGELOG.md')
        if ($null -eq $zipChangelogEntry) {
            throw 'Final-Link ZIP is missing root CHANGELOG.md.'
        }
        $reader = [System.IO.StreamReader]::new($zipChangelogEntry.Open())
        try {
            if ($reader.ReadToEnd() -cne $changelog) {
                throw 'Final-Link ZIP changelog differs from the verified package changelog.'
            }
        }
        finally {
            $reader.Dispose()
        }
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

function Assert-StableVersion {
    param([string]$RemotePath, [string]$Version, [string]$Commit)
    $lines = Invoke-Checked -FilePath $script:Ssh -Arguments @(
        $RemoteHost,
        "if test -f $RemotePath; then unzip -p $RemotePath kit-manifest.json; else echo NO_STABLE; fi"
    ) -Capture
    $raw = ($lines -join [Environment]::NewLine).Trim().TrimStart([char]0xFEFF)
    if ($raw -eq 'NO_STABLE') { return }
    $stable = $raw | ConvertFrom-Json
    $pattern = '^([0-9]+\.[0-9]+\.[0-9]+)\+([0-9]+)$'
    if ($stable.product -ne 'Alera' -or $stable.sourceCommit -notmatch '^[0-9a-f]{40}$' -or $stable.version -notmatch $pattern) {
        throw 'Cannot verify the existing stable Final-Link identity; refusing to replace it.'
    }
    $stableSemVer = [version]$Matches[1]
    $stableBuild = [int]$Matches[2]
    if ($Version -notmatch $pattern) { throw 'Invalid target version.' }
    $targetSemVer = [version]$Matches[1]
    $targetBuild = [int]$Matches[2]
    if ($AllowSamePublishedCommit -and $stable.version -eq $Version -and $stable.sourceCommit -eq $Commit) {
        Write-Host 'Stable package already has the same version and source; allowing explicit pending-release recovery.'
        return
    }
    if ($targetSemVer -le $stableSemVer -or $targetBuild -le $stableBuild) {
        throw "Stable version $($stable.version) is not older than $Version. Refusing downgrade or version reuse."
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

function Invoke-FileServerCommand([string]$Command) {
    return @(Invoke-Checked -FilePath $script:Ssh -Arguments @(
        $RemoteHost, $Command
    ) -Capture)
}

function Get-FileServerState {
    $state = (Invoke-FileServerCommand "timeout 10s docker inspect --format '{{.State.Status}}' $ContainerName") -join ''
    return $state.Trim()
}

function Assert-ServedPackage([string]$ExpectedHash) {
    $containerPath = '/app/uploads/alera/Alera-Final-Link-Kit.zip'
    $state = Get-FileServerState
    if ($state -in @('exited','created')) {
        Write-Warning "File server is $state; starting the existing container once (no restart)."
        try {
            Invoke-FileServerCommand "timeout 25s docker start $ContainerName" | Out-Null
        } catch {
            Write-Warning 'Container start did not return success; checking actual service state before deciding.'
        }
    } elseif ($state -notin @('running','restarting')) {
        throw "File server state '$state' requires manual investigation. No restart attempted."
    }
    $last = 'Service not yet verified.'
    for ($attempt = 1; $attempt -le 5; $attempt++) {
        try {
            if ((Get-FileServerState) -ne 'running') { throw 'File server is not running.' }
            $hashText = (Invoke-FileServerCommand "timeout 10s docker exec $ContainerName sha256sum $containerPath") -join "\n"
            $match = [regex]::Match($hashText,'(?im)^([0-9a-f]{64})\s+/app/uploads/alera/Alera-Final-Link-Kit\.zip\s*$')
            if (-not $match.Success -or $match.Groups[1].Value -ne $ExpectedHash) {
                throw 'Container-visible package hash does not match the verified archive.'
            }
            $login = ((Invoke-FileServerCommand "curl -sS --max-time 8 -o /dev/null -w '%{http_code}' http://127.0.0.1:3000/login") -join '').Trim()
            if ($login -ne '200') { throw "File server login HTTP status: $login" }
            $download = ((Invoke-FileServerCommand "curl -sS --max-time 8 -o /dev/null -w '%{http_code}' http://127.0.0.1:3000/download/alera/Alera-Final-Link-Kit.zip") -join '').Trim()
            if ($download -ne '302') { throw "Protected download route HTTP status: $download" }
            Write-Host "File server: running; login HTTP $login; protected download HTTP $download; container SHA256 $ExpectedHash"
            return
        } catch { $last = $_.Exception.Message }
        if ($attempt -lt 5) { Start-Sleep -Seconds 2 }
    }
    throw "Published archive may be present, but file-server verification failed: $last. No forced restart performed."
}

function Assert-PublishedIdentity([string]$RemotePath,[string]$Version,[string]$Commit,[string]$Hash) {
    $raw = ((Invoke-FileServerCommand "timeout 15s unzip -p $RemotePath kit-manifest.json") -join [Environment]::NewLine).Trim().TrimStart([char]0xFEFF)
    $manifest = $raw | ConvertFrom-Json
    if ($manifest.product -ne 'Alera' -or $manifest.version -ne $Version -or $manifest.sourceCommit -ne $Commit) {
        throw 'Published manifest does not match the pending release. Refusing recovery.'
    }
    if ((Get-RemoteHash -RemotePath $RemotePath) -ne $Hash) { throw 'Published archive SHA-256 differs from validated local artifact.' }
    Assert-ServedPackage $Hash
    Write-Host "Verified existing publication: $Version ($Commit), ZIP_OK, SHA256 $Hash"
}

function Write-PublishReceipt($Artifact) {
    if (-not $ReceiptPath) { return }
    $receipt = [pscustomobject]@{
        schemaVersion=1
        version=$Artifact.Version
        commit=$Artifact.Commit
        sha256=$Artifact.Sha256
        size=$Artifact.Size
    }
    $temp = "$ReceiptPath.$PID.tmp"
    [IO.File]::WriteAllText($temp,($receipt | ConvertTo-Json -Depth 5),[Text.UTF8Encoding]::new($false))
    if ([IO.File]::Exists($ReceiptPath)) { [IO.File]::Replace($temp,$ReceiptPath,$null) }
    else { [IO.File]::Move($temp,$ReceiptPath) }
}

if (-not [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform(
    [System.Runtime.InteropServices.OSPlatform]::Windows
)) {
    throw 'This release publisher only supports Windows.'
}

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$script:RepoRoot = (Resolve-Path -LiteralPath (Join-Path $scriptDir '..\..')).Path
if ($VerifyPublishedOnly) {
    if ($ExpectedCommit -notmatch '^[0-9a-f]{40}$' -or $ExpectedVersion -notmatch '^[0-9]+\.[0-9]+\.[0-9]+\+[0-9]+$' -or -not $ExpectedSha256) {
        throw 'VerifyPublishedOnly requires exact commit, version and a previously validated local SHA-256.'
    }
    $script:Ssh = (Get-Command ssh.exe -ErrorAction Stop).Source
    Assert-PublishedIdentity -RemotePath "$RemoteDirectory/Alera-Final-Link-Kit.zip" -Version $ExpectedVersion -Commit $ExpectedCommit -Hash $ExpectedSha256
    return
}
if ($env:CI -ne 'true') {
    & (Join-Path $script:RepoRoot 'tool\development\prepare_windows_build_storage.ps1') -RepoRoot $script:RepoRoot
}
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
    if ($ExpectedCommit -and $commit -ne $ExpectedCommit) { throw "Main changed: expected $ExpectedCommit, found $commit." }
    if ($ExpectedCommit -and $UseCurrentWorktree) { throw 'Pinned release requires an isolated worktree.' }
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
        $releaseWorktreeRoot = if ($env:CI -eq 'true') {
            Join-Path $script:RepoRoot '.worktrees'
        }
        else {
            if ([string]::IsNullOrWhiteSpace($env:ALERA_BUILD_STORAGE_ROOT)) {
                throw 'ALERA_BUILD_STORAGE_ROOT was not set by build storage preparation.'
            }
            Join-Path $env:ALERA_BUILD_STORAGE_ROOT 'worktrees'
        }
        New-Item -ItemType Directory -Path $releaseWorktreeRoot -Force | Out-Null
        $releaseWorktree = Join-Path $releaseWorktreeRoot "release-auto-$shortCommit-$PID"
        Write-Step 'Create isolated pinned release worktree'
        Invoke-Checked -FilePath 'git' -Arguments @('-C', $script:RepoRoot, 'worktree', 'add', '--detach', $releaseWorktree, $commit)
        $createdReleaseWorktree = $true
        $script:BuildRoot = (Resolve-Path -LiteralPath $releaseWorktree).Path
        Restore-PinnedSubmodule -RelativePath 'third_party/xterm' -Commit $commit
        Restore-PinnedSubmodule -RelativePath 'third_party/dart_terminal' -Commit $commit
        if ($env:CI -ne 'true') {
            & (Join-Path $script:RepoRoot 'tool\development\prepare_windows_build_storage.ps1') -RepoRoot $script:BuildRoot
        }
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
    if ($ExpectedVersion -and $artifact.Version -ne $ExpectedVersion) { throw "Artifact version mismatch: expected $ExpectedVersion, found $($artifact.Version)." }
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

    if ($ProtectStableVersion) { Assert-StableVersion -RemotePath $remoteStable -Version $artifact.Version -Commit $commit }

    Write-Step 'Sync to remote staging path'
    Sync-RemoteStaging -LocalPath $artifact.ZipPath -RemoteStable $remoteStable -RemoteStaging $remoteStaging
    $stagingHash = Get-RemoteHash -RemotePath $remoteStaging
    if ($stagingHash -ne $artifact.Sha256) {
        throw "Remote staging hash mismatch. Local $($artifact.Sha256), remote $stagingHash."
    }

    if ($ProtectStableVersion) { Assert-StableVersion -RemotePath $remoteStable -Version $artifact.Version -Commit $commit }

    Write-Step 'Atomically promote and verify remote stable package'
    Invoke-Checked -FilePath $script:Ssh -Arguments @($RemoteHost, "mv -f $remoteStaging $remoteStable")
    $stableHash = Get-RemoteHash -RemotePath $remoteStable
    if ($stableHash -ne $artifact.Sha256) {
        throw "Remote stable hash mismatch. Local $($artifact.Sha256), remote $stableHash."
    }

    # Persist the verified promotion before checking service availability. A retry
    # can verify the same bytes without another build, upload, or version bump.
    Write-PublishReceipt $artifact
    Write-Step 'Verify file-server and served package without restarting'
    Assert-ServedPackage $artifact.Sha256
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
