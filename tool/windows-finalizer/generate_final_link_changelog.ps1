[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$RepoRoot,
    [Parameter(Mandatory = $true)][string]$OutputDirectory,
    [Parameter(Mandatory = $true)][string]$Version,
    [Parameter(Mandatory = $true)][string]$SourceCommit
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$parsedVersion = [regex]::Match($Version, '^(?<release>[0-9]+\.[0-9]+\.[0-9]+)\+(?<build>[0-9]+)$')
if (-not $parsedVersion.Success) {
    throw "Invalid Final-Link version: $Version"
}

$releaseFile = Join-Path $RepoRoot 'tool\release\prepared_release.json'
if (-not (Test-Path -LiteralPath $releaseFile -PathType Leaf)) {
    throw "Prepared release metadata was not found: $releaseFile"
}
$prepared = Get-Content -LiteralPath $releaseFile -Raw | ConvertFrom-Json
$releaseVersion = $parsedVersion.Groups['release'].Value
$releaseTag = "v$releaseVersion"
if ($prepared.desktop.releaseVersion -ne $releaseVersion -or
    [int]$prepared.desktop.buildNumber -ne [int]$parsedVersion.Groups['build'].Value -or
    $prepared.desktop.tag -ne $releaseTag) {
    throw "Prepared desktop release metadata does not match Final-Link version $Version."
}
$previousTag = [string]$prepared.desktop.previousTag
if ([string]::IsNullOrWhiteSpace($previousTag) -or $previousTag -eq $releaseTag) {
    throw 'Prepared desktop previousTag is missing or matches the current release.'
}

$head = (& git -C $RepoRoot rev-parse HEAD)
if ($LASTEXITCODE -ne 0 -or @($head).Count -ne 1) {
    throw 'Could not resolve Final-Link source HEAD.'
}
$head = ([string]$head).Trim()
if ($head -ne $SourceCommit) {
    throw "Final-Link source HEAD mismatch: expected $SourceCommit, found $head."
}

# Release tags are not guaranteed to be pushed. Resolve the previous release
# on the selected source history rather than trusting local/remote tag refs.
$history = @(& git -C $RepoRoot log --first-parent '--format=%H%x09%s' $SourceCommit)
if ($LASTEXITCODE -ne 0) { throw 'Could not read release history.' }
$previousCommit = $null
foreach ($line in $history) {
    $parts = ([string]$line).Split(@("`t"), 2, [System.StringSplitOptions]::None)
    if ($parts.Count -eq 2 -and $parts[1] -eq "release: $previousTag") {
        $previousCommit = $parts[0]
        break
    }
}
if (-not $previousCommit) {
    throw "Cannot find previous release $previousTag in source history; refusing to write a misleading changelog."
}

$changeLines = @(& git -C $RepoRoot log --no-merges '--format=%h%x09%s' "$previousCommit..$SourceCommit")
if ($LASTEXITCODE -ne 0) { throw "Could not read changes since $previousTag." }
$changes = [System.Collections.Generic.List[string]]::new()
$tick = [char]96
foreach ($line in $changeLines) {
    $parts = ([string]$line).Split(@("`t"), 2, [System.StringSplitOptions]::None)
    if ($parts.Count -ne 2 -or $parts[1] -match '^release: v[0-9]+\.[0-9]+\.[0-9]+$') { continue }
    $subject = $parts[1].Replace([string]$tick, "'").Trim()
    if ($subject) { $changes.Add("- $tick$($parts[0])$tick $subject") }
}
if ($changes.Count -eq 0) {
    $changes.Add('- No non-release commits since the previous release.')
}

$lines = @(
    "# Alera Windows Final-Link Kit - $Version",
    '',
    "- **Version:** $tick$Version$tick",
    "- **Release:** $tick$releaseTag$tick",
    "- **Build number:** $tick$($parsedVersion.Groups['build'].Value)$tick",
    "- **Source commit:** $tick$SourceCommit$tick",
    "- **Previous release:** $tick$previousTag$tick",
    '- **Package:** Windows x64 Release (semi-compiled Final-Link Kit)',
    '',
    "## Changes since $previousTag",
    ''
) + @($changes.ToArray()) + @(
    '',
    'The version above identifies this ZIP source build, not an Alera executable already installed on another computer.',
    'For machine-readable metadata, see `kit-manifest.json`.',
    ''
)
if (-not (Test-Path -LiteralPath $OutputDirectory -PathType Container)) {
    New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
}
$target = Join-Path $OutputDirectory 'CHANGELOG.md'
[System.IO.File]::WriteAllText($target, ($lines -join "`n"), [System.Text.UTF8Encoding]::new($false))
Write-Host "Final-Link changelog: $target ($Version, $SourceCommit; $($changes.Count) entries)"
