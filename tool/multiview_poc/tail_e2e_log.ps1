param(
  [string]$Configuration = 'Debug',
  [string]$Pattern = 'RangeError|reflow|SEVERE|disconnected|Unhandled|ERROR',
  [int]$Tail = 12
)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$dir = if ($Configuration -eq 'Release') {
  Join-Path $root 'build\windows\shared_release_ninja\e2e_support\logs'
} else {
  Join-Path $root 'build\windows\shared_gui_ninja\e2e_support\logs'
}
$log = Get-ChildItem (Join-Path $dir '*.log') | Sort-Object LastWriteTime | Select-Object -Last 1
if (-not $log) { throw "no log under $dir" }
Write-Output "== $($log.Name) (lines=$((Get-Content $log.FullName).Count))"
Select-String -Path $log.FullName -Pattern $Pattern | Select-Object -Last $Tail | ForEach-Object {
  Write-Output ("{0}: {1}" -f $_.LineNumber, $_.Line.Trim())
}
