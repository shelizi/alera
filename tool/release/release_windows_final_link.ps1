[CmdletBinding()]
param(
    [switch]$Plan,
    [ValidatePattern('^[0-9A-Fa-f]{64}$')][string]$RecoverPublishedSha256
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$paths = @('pubspec.yaml','tool/release/prepared_release.json')
$pub = Join-Path $root $paths[0]
$meta = Join-Path $root $paths[1]
$publisher = Join-Path $PSScriptRoot 'publish_windows_final_link.ps1'
$utf8 = [Text.UTF8Encoding]::new($false)

function Git-Text([string[]]$Arguments) {
    $out = @(& git.exe -C $root @Arguments)
    if ($LASTEXITCODE -ne 0) { throw "Git failed: $($Arguments -join ' ')" }
    return ($out -join [Environment]::NewLine).Trim()
}
function Assert-Inputs {
    if ((Git-Text @('branch','--show-current')) -ne 'main') { throw 'Run from local main.' }
    if (Git-Text @('diff','--cached','--name-only')) { throw 'Staged changes exist; refusing to modify them.' }
    if (Git-Text (@('status','--porcelain','--') + $paths + @('tool/release/release_windows_final_link.ps1','tool/release/publish_windows_final_link.ps1'))) {
        throw 'Release files have uncommitted changes; inspect or commit them first.'
    }
    foreach ($operation in @('MERGE_HEAD','CHERRY_PICK_HEAD','REVERT_HEAD','rebase-merge','rebase-apply','sequencer')) {
        $path = Git-Text @('rev-parse','--git-path',$operation)
        if (-not [IO.Path]::IsPathRooted($path)) { $path = Join-Path $root $path }
        if (Test-Path -LiteralPath $path) { throw "Git operation in progress: $operation" }
    }
    if (-not (Test-Path -LiteralPath $publisher -PathType Leaf)) { throw 'Publisher missing.' }
}
function Read-Version {
    $content = [IO.File]::ReadAllText($pub)
    $found = [regex]::Matches($content,'(?m)^version: ([0-9]+\.[0-9]+\.[0-9]+)\+([0-9]+)(?=\r?$)')
    if ($found.Count -ne 1) { throw 'Expected one desktop version: major.minor.patch+build.' }
    return [pscustomobject]@{ SemVer=[version]$found[0].Groups[1].Value; Build=[int]$found[0].Groups[2].Value; Text=$found[0].Groups[1].Value+'+'+$found[0].Groups[2].Value; Content=$content }
}
function Read-Metadata {
    $m = [IO.File]::ReadAllText($meta) | ConvertFrom-Json
    if ($m.schemaVersion -ne 1 -or $null -eq $m.desktop -or $null -eq $m.mobile) { throw 'Invalid prepared metadata.' }
    return $m
}
function Previous-Tag([string]$Source) {
    $subject = Git-Text @('log','--first-parent','--format=%s','--grep=^release: v','-1',$Source)
    if ($subject -notmatch '^release: (v[0-9]+\.[0-9]+\.[0-9]+)$') { throw 'No first-parent release marker.' }
    return $Matches[1]
}
function Assert-Prepared($p,[string]$head,[switch]$Historical) {
    if (-not $Historical -and (Git-Text @('rev-parse','HEAD')) -ne $head) { throw 'Main changed.' }
    $v = Git-Text @('show',"$($head):pubspec.yaml")
    $m = (Git-Text @('show',"$($head):tool/release/prepared_release.json")) | ConvertFrom-Json
    $versions = [regex]::Matches($v,'(?m)^version: ([0-9]+\.[0-9]+\.[0-9]+\+[0-9]+)(?=\r?$)')
    if ((Git-Text @('rev-parse',"$head^")) -ne $p.sourceCommit -or
        (Git-Text @('log','-1','--format=%s',$head)) -ne "release: v$($p.version)" -or
        $versions.Count -ne 1 -or $versions[0].Groups[1].Value -ne "$($p.version)+$($p.build)" -or
        $m.sourceSha -ne $p.sourceCommit -or $m.desktop.shouldRelease -ne $true -or
        $m.desktop.artifactVersion -ne $p.version -or $m.desktop.releaseVersion -ne $p.version -or
        $m.desktop.buildNumber -ne $p.build -or $m.desktop.tag -ne "v$($p.version)" -or $m.desktop.previousTag -ne $p.previousTag) {
        throw 'Pending release does not match main; inspect before retrying. Never publish stale source.'
    }
    $changed = @((Git-Text @('diff-tree','--no-commit-id','--name-only','-r',$head)) -split '\r?\n')
    if ($changed.Count -ne 2 -or @($changed | Where-Object { $_ -notin $paths }).Count -gt 0) { throw 'Release commit has unexpected files.' }
}

function Complete-ExistingPublication($p,[string]$head,[string]$hash) {
    $target = "$($p.version)+$($p.build)"
    Write-Host "Recovering verified archive $target ($head) without build, upload, or restart."
    & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $publisher -VerifyPublishedOnly -ExpectedCommit $head -ExpectedVersion $target -ExpectedSha256 $hash
    if ($LASTEXITCODE -ne 0) { throw 'Existing publication verification failed; retaining pending release and receipt.' }
    [IO.File]::Delete($pendingPath)
    Write-Host "Verified internal publication: $target ($head)"
    Write-Host 'No Git push, tag or GitHub Release performed.'
}

function Get-ReleasePlan {
    Assert-Inputs
    $head = Git-Text @('rev-parse','HEAD')
    $v = Read-Version
    $m = Read-Metadata
    $next = "$($v.SemVer.Major).$($v.SemVer.Minor).$($v.SemVer.Build+1)"
    $build = [int]($v.Build+1)
    $existing = Test-Path -LiteralPath $pendingPath -PathType Leaf
    $prepared = $false
    if ($existing) {
        $p = [IO.File]::ReadAllText($pendingPath) | ConvertFrom-Json
        if ($p.schemaVersion -ne 1 -or $p.sourceCommit -notmatch '^[0-9a-f]{40}$' -or $p.version -notmatch '^[0-9]+\.[0-9]+\.[0-9]+$' -or $p.build -le 0 -or $p.previousTag -notmatch '^v[0-9]+\.[0-9]+\.[0-9]+$') { throw 'Invalid pending release record.' }
        if ($head -ne $p.sourceCommit) { Assert-Prepared $p $head; $prepared = $true }
        elseif ($p.version -ne $next -or $p.build -ne $build -or $p.previousTag -ne (Previous-Tag $head)) { throw 'Pending plan no longer matches source.' }
    } else {
        if ([version]$m.desktop.releaseVersion -gt $v.SemVer -or [int]$m.desktop.buildNumber -gt $v.Build) { throw 'Prepared metadata is newer than pubspec; resolve before release.' }
        $p = [pscustomobject]@{schemaVersion=1;sourceCommit=$head;version=$next;build=$build;previousTag=(Previous-Tag $head)}
    }
    if (Git-Text @('tag','--list',"v$($p.version)")) { throw 'Target local tag already exists.' }
    return [pscustomobject]@{Pending=$p;Existing=$existing;Prepared=$prepared;Head=$head}
}

$gitDir = Git-Text @('rev-parse','--git-common-dir')
if (-not [IO.Path]::IsPathRooted($gitDir)) { $gitDir = Join-Path $root $gitDir }
$gitDir = (Resolve-Path $gitDir).Path
$pendingPath = Join-Path $gitDir 'alera-final-link-pending.json'
$lockPath = Join-Path $gitDir 'alera-final-link-release.lock'
if ($Plan -and $RecoverPublishedSha256) { throw 'Plan and explicit recovery are mutually exclusive.' }
if ($Plan) { Get-ReleasePlan | ConvertTo-Json -Depth 8; return }

$lock = $null
try {
    try { $lock = [IO.File]::Open($lockPath,[IO.FileMode]::OpenOrCreate,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None) }
    catch { throw 'Another one-command release is running.' }
    if ($RecoverPublishedSha256) {
        Assert-Inputs
        if (-not (Test-Path -LiteralPath $pendingPath -PathType Leaf)) { throw 'No pending release to recover.' }
        $p = [IO.File]::ReadAllText($pendingPath) | ConvertFrom-Json
        if ($p.schemaVersion -ne 1 -or $p.sourceCommit -notmatch '^[0-9a-f]{40}$' -or $p.version -notmatch '^[0-9]+\.[0-9]+\.[0-9]+$' -or $p.build -le 0 -or $p.previousTag -notmatch '^v[0-9]+\.[0-9]+\.[0-9]+$') { throw 'Invalid pending release record.' }
        # Reconcile an exact ancestor release, even if main advanced during repair.
        $candidates = @((Git-Text @('log','--format=%H','--fixed-strings',"--grep=release: v$($p.version)","$($p.sourceCommit)..HEAD")) -split '\r?\n' |
            Where-Object { $_ -match '^[0-9a-f]{40}$' -and (Git-Text @('rev-parse',"$_^")) -eq $p.sourceCommit -and (Git-Text @('log','-1','--format=%s',$_)) -eq "release: v$($p.version)" })
        if ($candidates.Count -ne 1) { throw 'Cannot uniquely identify the pending release commit in main history.' }
        $historical = $candidates[0]
        Assert-Prepared $p $historical -Historical
        Complete-ExistingPublication $p $historical $RecoverPublishedSha256
        return
    }
    $r = Get-ReleasePlan
    $p = $r.Pending
    Write-Host "Final-Link target: $($p.version)+$($p.build)"
    if (-not $r.Existing) {
        $temp = "$pendingPath.$PID.tmp"
        [IO.File]::WriteAllText($temp,($p | ConvertTo-Json -Depth 8),$utf8)
        [IO.File]::Move($temp,$pendingPath)
    }
    if (-not $r.Prepared) {
        Assert-Inputs
        if ((Git-Text @('rev-parse','HEAD')) -ne $p.sourceCommit) { throw 'Main advanced before preparation.' }
        $v = Read-Version
        $m = Read-Metadata
        $updated = [regex]::Replace($v.Content,'(?m)^version: [0-9]+\.[0-9]+\.[0-9]+\+[0-9]+(?=\r?$)',"version: $($p.version)+$($p.build)")
        $m.sourceSha=$p.sourceCommit
        $m.desktop.shouldRelease=$true
        $m.desktop.artifactVersion=$p.version
        $m.desktop.releaseVersion=$p.version
        $m.desktop.buildNumber=$p.build
        $m.desktop.tag="v$($p.version)"
        $m.desktop.previousTag=$p.previousTag
        [IO.File]::WriteAllText($pub,$updated,$utf8)
        [IO.File]::WriteAllText($meta,(($m | ConvertTo-Json -Depth 32)+[Environment]::NewLine),$utf8)
        Git-Text (@('diff','--check','--')+$paths) | Out-Null
        if ((Git-Text @('rev-parse','HEAD')) -ne $p.sourceCommit -or (Git-Text @('diff','--cached','--name-only'))) { throw 'Git changed before commit; inspect pending files.' }
        Write-Host (Git-Text (@('commit','--only','-m',"release: v$($p.version)",'--')+$paths))
    } else { Write-Host 'Resuming prepared release; no version bump.' }
    $head = Git-Text @('rev-parse','HEAD')
    Assert-Inputs
    Assert-Prepared $p $head
    Write-Host "Release commit: $head"
    $argsForPublisher = @('-NoProfile','-ExecutionPolicy','Bypass','-File',$publisher,'-ExpectedCommit',$head,'-ExpectedVersion',"$($p.version)+$($p.build)",'-ProtectStableVersion')
    $receiptPath = Join-Path $gitDir "alera-final-link-receipt-$($p.version)-$head.json"
    if ($r.Existing -and (Test-Path -LiteralPath $receiptPath -PathType Leaf)) {
        $receipt = [IO.File]::ReadAllText($receiptPath) | ConvertFrom-Json
        if ($receipt.schemaVersion -ne 1 -or $receipt.commit -ne $head -or
            $receipt.version -ne "$($p.version)+$($p.build)" -or $receipt.sha256 -notmatch '^[0-9a-f]{64}$' -or $receipt.size -le 0) {
            throw 'Publication receipt does not match the pending release. Inspect before retrying.'
        }
        Complete-ExistingPublication $p $head $receipt.sha256
        return
    }
    $argsForPublisher += @('-ReceiptPath',$receiptPath)
    if ($r.Existing) { $argsForPublisher += '-AllowSamePublishedCommit' }
    & powershell.exe @argsForPublisher
    if ($LASTEXITCODE -ne 0) { throw "Final-Link publisher failed ($LASTEXITCODE). Re-run to resume same version." }
    [IO.File]::Delete($pendingPath)
    Write-Host "Verified internal publication: $($p.version)+$($p.build) ($head)"
    Write-Host 'No Git push, tag or GitHub Release performed.'
    Write-Host (Git-Text @('status','--short','--branch','--untracked-files=no'))
} finally { if ($null -ne $lock) { $lock.Dispose() } }
