param(
  [ValidateRange(1,5000)][int]$Count=1200,
  [ValidateRange(0,500)][int]$DelayMs=20
)
$ErrorActionPreference='Stop'

# This script is copied into and run ONLY from the disposable Alera E2E
# workspace. Never create progress artifacts in a real user's shell/workspace.
$cwd=(Get-Location).Path
if($cwd -notmatch '[\\/]shared_gui_ninja[\\/]e2e_workspace$') {
  throw "Not an isolated Alera E2E terminal workspace: $cwd"
}

$progressFile=Join-Path $cwd 'terminal_stream_progress.txt'
$resultFile=Join-Path $cwd 'terminal_stream_result.json'
$started=[DateTimeOffset]::UtcNow
$linePayload='X'*48
[IO.File]::WriteAllText($progressFile,'0')
Write-Output "E2E_STREAM_BEGIN COUNT=$Count PID=$PID"

for($i=1;$i -le $Count;$i++) {
  Write-Output ('E2E_STREAM_{0:D4}_{1}' -f $i,$linePayload)
  if(($i % 50) -eq 0) {
    [IO.File]::WriteAllText($progressFile,"$i")
  }
  if($DelayMs -gt 0) { Start-Sleep -Milliseconds $DelayMs }
}

$ended=[DateTimeOffset]::UtcNow
[IO.File]::WriteAllText($progressFile,"$Count")
[pscustomobject]@{
  completed=$true
  count=$Count
  scriptPid=$PID
  startedAt=$started.ToString('o')
  finishedAt=$ended.ToString('o')
  durationMs=[math]::Round(($ended-$started).TotalMilliseconds)
} | ConvertTo-Json -Compress | Set-Content -Path $resultFile -Encoding utf8
Write-Output "E2E_STREAM_COMPLETE COUNT=$Count"
