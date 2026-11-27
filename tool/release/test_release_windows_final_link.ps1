[CmdletBinding()]
param([Parameter(Mandatory=$true)][string]$Fixture)
Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'
$repo=(Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$fixtureRoot=Join-Path $repo '.worktrees'
$fixture=[IO.Path]::GetFullPath((Join-Path $repo $Fixture))
if (-not $fixture.StartsWith($fixtureRoot+'\',[StringComparison]::OrdinalIgnoreCase) -or (Test-Path $fixture)) { throw 'Use a new ignored .worktrees test directory.' }
$checks=0
function Assert-True($ok,[string]$message) {
    if (-not $ok) { throw "FAIL: $message" }
    $script:checks++
    Write-Host "PASS: $message"
}
function Git([string[]]$Arguments) {
    $out=@(& git.exe -C $fixture @Arguments)
    if ($LASTEXITCODE -ne 0) { throw "Fixture git failed: $($Arguments -join ' ')" }
    return ($out -join [Environment]::NewLine).Trim()
}
function Run-Release([string[]]$Arguments=@(),[switch]$ExpectFailure) {
    $ErrorActionPreference='Continue'
    $out=@(& powershell.exe -NoProfile -ExecutionPolicy Bypass -File $entry @Arguments 2>&1)
    $code=$LASTEXITCODE
    $msg=$out -join [Environment]::NewLine
    if ($ExpectFailure) { if ($code -eq 0) { throw "Expected failure: $msg" } }
    elseif ($code -ne 0) { throw "Unexpected failure: $msg" }
    return $msg
}
function Write-Text([string]$Path,[string]$Text) { [IO.File]::WriteAllText($Path,$Text,[Text.UTF8Encoding]::new($false)) }

New-Item -ItemType Directory -Path (Join-Path $fixture 'tool\release') -Force | Out-Null
Git @('init','-b','main') | Out-Null
Git @('config','user.name','Release Test') | Out-Null
Git @('config','user.email','release-test@example.invalid') | Out-Null
Git @('config','commit.gpgsign','false') | Out-Null
$entry=Join-Path $fixture 'tool\release\release_windows_final_link.ps1'
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'release_windows_final_link.ps1') -Destination $entry
$stub=Join-Path $fixture 'tool\release\publish_windows_final_link.ps1'
Write-Text $stub @'
param([string]$ExpectedCommit,[string]$ExpectedVersion,[switch]$ProtectStableVersion,[switch]$AllowSamePublishedCommit)
$root=(Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
if (-not $ProtectStableVersion -or $ExpectedCommit -ne ((git.exe -C $root rev-parse HEAD).Trim())) { exit 12 }
$m=Get-Content (Join-Path $root 'tool\release\prepared_release.json') -Raw | ConvertFrom-Json
if ($ExpectedVersion -ne "$($m.desktop.releaseVersion)+$($m.desktop.buildNumber)") { exit 13 }
if ($env:ALERA_TEST_PUBLISH_FAIL -eq '1') { exit 19 }
if ($env:ALERA_TEST_EXPECT_RESUME -eq '1' -and -not $AllowSamePublishedCommit) { exit 20 }
exit 0
'@
$pub=Join-Path $fixture 'pubspec.yaml'
$meta=Join-Path $fixture 'tool\release\prepared_release.json'
Write-Text $pub "name: alera$([Environment]::NewLine)version: 1.2.3+8$([Environment]::NewLine)"
Write-Text $meta '{"schemaVersion":1,"sourceSha":"old","channel":"stable","desktop":{"shouldRelease":true,"artifactVersion":"1.2.2","releaseVersion":"1.2.2","buildNumber":7,"tag":"v1.2.2","previousTag":"v1.2.1"},"mobile":{"shouldRelease":false,"artifactVersion":"9.0.0","buildNumber":999}}'
$dirty=Join-Path $fixture 'notes.txt'
Write-Text $dirty 'original'
Git @('add','.') | Out-Null
Git @('commit','-m','release: v1.2.2') | Out-Null
$before=Git @('rev-parse','HEAD')
Write-Text $dirty 'uncommitted user work'
Write-Text (Join-Path $fixture 'untracked.txt') 'keep'
$gitDir=Git @('rev-parse','--absolute-git-dir')
$pending=Join-Path $gitDir 'alera-final-link-pending.json'
$preview=Run-Release @('-Plan')
Assert-True ($preview -match '"version":\s*"1.2.4"' -and (Git @('rev-parse','HEAD')) -eq $before -and -not (Test-Path $pending)) 'plan is read-only and handles stale metadata'
$env:ALERA_TEST_PUBLISH_FAIL='1'
$failed=Run-Release -ExpectFailure
$prepared=Git @('rev-parse','HEAD')
Assert-True ($failed -match 'publisher failed' -and $prepared -ne $before -and (Test-Path $pending)) "failed publisher retains commit and pending record: $failed"
$m=Get-Content $meta -Raw | ConvertFrom-Json
Assert-True ($m.sourceSha -eq $before -and $m.desktop.tag -eq 'v1.2.4' -and $m.desktop.buildNumber -eq 9 -and $m.desktop.previousTag -eq 'v1.2.2') 'release identity and first-parent marker match'
Assert-True ($m.mobile.artifactVersion -eq '9.0.0' -and $m.mobile.buildNumber -eq 999 -and (Get-Content $dirty -Raw) -eq 'uncommitted user work' -and (Test-Path (Join-Path $fixture 'untracked.txt'))) 'mobile and preexisting user files preserved'
$env:ALERA_TEST_PUBLISH_FAIL='0'
$env:ALERA_TEST_EXPECT_RESUME='1'
$null=Run-Release
Assert-True ((Git @('rev-parse','HEAD')) -eq $prepared -and -not (Test-Path $pending)) 'retry uses same commit and clears pending state'
$env:ALERA_TEST_EXPECT_RESUME='0'
$null=Run-Release
$second=Git @('rev-parse','HEAD')
Assert-True ($second -ne $prepared -and (Get-Content $pub -Raw) -match 'version: 1.2.5\+10') 'new run after success increments version'
Assert-True ((Git @('tag','--list')) -eq '' -and (Git @('remote')) -eq '') 'no tags or remotes'
Git @('add','notes.txt') | Out-Null
$null=Run-Release -ExpectFailure
Assert-True ((Git @('rev-parse','HEAD')) -eq $second -and (Git @('diff','--cached','--name-only')) -eq 'notes.txt') 'staged changes block release'
Git @('restore','--staged','notes.txt') | Out-Null
$lock=[IO.File]::Open((Join-Path $gitDir 'alera-final-link-release.lock'),[IO.FileMode]::OpenOrCreate,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
try {
    $blocked=Run-Release -ExpectFailure
    Assert-True ($blocked -match 'Another one-command' -and (Git @('rev-parse','HEAD')) -eq $second) 'concurrent run blocked'
} finally { $lock.Dispose() }
Git @('checkout','-b','other') | Out-Null
$null=Run-Release -ExpectFailure
Assert-True ((Git @('rev-parse','HEAD')) -eq $second) 'non-main rejected'
Git @('checkout','main') | Out-Null
$env:ALERA_TEST_PUBLISH_FAIL='1'
$null=Run-Release -ExpectFailure
$third=Git @('rev-parse','HEAD')
Git @('commit','--allow-empty','-m','chore: advance main') | Out-Null
$advanced=Git @('rev-parse','HEAD')
$env:ALERA_TEST_PUBLISH_FAIL='0'
$null=Run-Release -ExpectFailure
Assert-True ($advanced -ne $third -and (Git @('rev-parse','HEAD')) -eq $advanced -and (Test-Path $pending)) 'advanced main cannot publish stale pending commit'
$tokens=$null; $errors=$null
$ast=[Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot 'publish_windows_final_link.ps1'),[ref]$tokens,[ref]$errors)
Assert-True ($errors.Count -eq 0) 'publisher parses'
$guard=$ast.Find({param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Assert-StableVersion'},$true)
if ($null -eq $guard) { throw 'Missing stable version guard.' }
. ([scriptblock]::Create($guard.Extent.Text))
$script:Ssh='fake'; $RemoteHost='fake'
$same='a'*40; $other='b'*40
$script:stableRaw='{"product":"Alera","version":"1.2.4+9","sourceCommit":"'+$same+'"}'
function Invoke-Checked { param($FilePath,$Arguments,[switch]$Capture) return $script:stableRaw }
$AllowSamePublishedCommit=$false
Assert-StableVersion -RemotePath '/fake' -Version '1.2.5+10' -Commit $other
Assert-True $true 'newer version allowed'
foreach ($version in @('1.2.3+8','1.2.4+9','1.2.5+9')) {
    $rejected=$false
    try { Assert-StableVersion -RemotePath '/fake' -Version $version -Commit $other } catch { $rejected=$true }
    Assert-True $rejected "stale/reused version $version rejected"
}
$AllowSamePublishedCommit=$true
Assert-StableVersion -RemotePath '/fake' -Version '1.2.4+9' -Commit $same
Assert-True $true 'same-version recovery only for matching commit'
$script:stableRaw='NO_STABLE'
Assert-StableVersion -RemotePath '/fake' -Version '1.2.4+9' -Commit $same
Assert-True $true 'first release permitted'
Write-Host "PASS: $checks checks; no actual build, SSH, upload or push."
Write-Host "Retained disposable fixture: $fixture"
