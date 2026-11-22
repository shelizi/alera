#requires -Version 7.2
<#
.SYNOPSIS
Compare two Alera Native (GPUI) Windows executables without using live app state.
.DESCRIPTION
Runs alternating baseline/candidate launches with isolated state copied from an
optional sanitized template. Only benchmark-launched processes are closed.
Window visibility and message-loop responsiveness are proxies, NOT first paint,
Editor input readiness, or authenticated Agent transcript restoration.
.EXAMPLE
pwsh -File tool/development/measure_windows_startup.ps1 -BaselineExe D:\alera-build\startup-benchmark-20261122\main.exe -CandidateExe D:\alera-build\startup-benchmark-20261122\candidate.exe -WorkspacePath E:\test-workspace -ScratchRoot D:\alera-build\startup-benchmark-20261122 -Runs 3
#>
[CmdletBinding()]
param(
  [Parameter(Mandatory)][string]$BaselineExe,
  [Parameter(Mandatory)][string]$CandidateExe,
  [Parameter(Mandatory)][string]$WorkspacePath,
  [Parameter(Mandatory)][string]$ScratchRoot,
  [string]$StateTemplateDir,
  [ValidateRange(1, 20)][int]$Runs = 3,
  [ValidateRange(1, 180)][int]$WindowTimeoutSeconds = 30,
  [ValidateRange(1, 600)][int]$ObservationSeconds = 10,
  [switch]$PlanOnly
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'Windows desktop measurement only' }

function Get-FullExistingPath([string]$PathValue, [string]$Kind) {
  $full = [IO.Path]::GetFullPath($PathValue)
  if (-not (Test-Path -LiteralPath $full -PathType $Kind)) {
    throw "Missing $Kind path: $full"
  }
  return $full
}

function Get-ProcessIo([int]$ProcessId) {
  try {
    $entry = Get-CimInstance -ClassName Win32_Process -Filter ("ProcessId = {0}" -f $ProcessId) -ErrorAction Stop
    if ($null -ne $entry) {
      return @{ read = [double]$entry.ReadTransferCount; write = [double]$entry.WriteTransferCount }
    }
  } catch {
    # WMI/CIM can be unavailable on restricted Windows installations.
  }
  return $null
}

function Get-Median([object[]]$Numbers) {
  $sorted = @($Numbers | Sort-Object)
  if ($sorted.Count -eq 0) { return $null }
  $middle = [int][Math]::Floor($sorted.Count / 2)
  if (($sorted.Count % 2) -eq 1) { return [Math]::Round([double]$sorted[$middle], 1) }
  return [Math]::Round(([double]$sorted[$middle - 1] + [double]$sorted[$middle]) / 2, 1)
}

$baseline = Get-FullExistingPath $BaselineExe 'Leaf'
$candidate = Get-FullExistingPath $CandidateExe 'Leaf'
$workspace = Get-FullExistingPath $WorkspacePath 'Container'
$scratch = [IO.Path]::GetFullPath($ScratchRoot).TrimEnd('\')
$srcState = $null
if ($StateTemplateDir) { $srcState = Get-FullExistingPath $StateTemplateDir 'Container' }
if ($scratch -match '(?i)[\\/]Dropbox[\\/]') { throw 'ScratchRoot must be outside Dropbox' }
if ($scratch.StartsWith($workspace.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase)) {
  throw 'ScratchRoot must be outside the measured Workspace'
}
if ($srcState -and $scratch.StartsWith($srcState.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase)) {
  throw 'ScratchRoot must not be nested inside the state template'
}
if ($baseline -eq $candidate) { throw 'Baseline and candidate executable paths must differ' }
$hashes = @{
  baseline = (Get-FileHash -LiteralPath $baseline -Algorithm SHA256).Hash
  candidate = (Get-FileHash -LiteralPath $candidate -Algorithm SHA256).Hash
}
if ($hashes.baseline -eq $hashes.candidate) {
  throw 'Identical binary hashes do not provide a useful A/B comparison'
}
if ($PlanOnly) {
  [pscustomobject]@{ baselineSha256=$hashes.baseline; candidateSha256=$hashes.candidate; runs=$Runs; stateTemplateProvided=($null -ne $srcState); observationSeconds=$ObservationSeconds; launches=($Runs*2) }
  return
}

New-Item -ItemType Directory -Path $scratch -Force | Out-Null
$observations = [Collections.Generic.List[object]]::new()
$session = (Get-Date).ToUniversalTime().ToString('yyyyMMddTHHmmssZ') + '-' + ([guid]::NewGuid().ToString('N').Substring(0,8))
foreach ($round in 1..$Runs) {
  # Reverse order every round to limit bias from filesystem and shader caches.
  $order = if (($round % 2) -eq 0) { @('candidate','baseline') } else { @('baseline','candidate') }
  foreach ($label in $order) {
    $exe = if ($label -eq 'baseline') { $baseline } else { $candidate }
    $state = Join-Path $scratch ("state-{0}-{1}-{2}" -f $session,$round,$label)
    New-Item -ItemType Directory -Path $state -Force | Out-Null
    if ($srcState) {
      Get-ChildItem -LiteralPath $srcState -Force | Copy-Item -Destination $state -Recurse -Force
    }
    $result = [ordered]@{
      round = $round
      variant = $label
      windowVisibleMs = $null
      windowResponsiveMs = $null
      cpuCoreSeconds = $null
      workingSetMB = $null
      privateMemoryMB = $null
      readMB = $null
      writeMB = $null
      error = $null
    }
    $process = $null
    try {
      $psi = [Diagnostics.ProcessStartInfo]::new($exe)
      $psi.UseShellExecute = $false
      $psi.WorkingDirectory = $workspace
      $psi.ArgumentList.Add($workspace)
      $psi.Environment['ALERA_GPUI_STATE_DIR'] = $state
      $psi.Environment['ALERA_GPUI_STARTUP_TRACE'] = '1'
      $watch = [Diagnostics.Stopwatch]::StartNew()
      $process = [Diagnostics.Process]::Start($psi)
      if ($null -eq $process) { throw 'Process.Start returned null' }
      $deadlineMs = $WindowTimeoutSeconds * 1000
      while ($watch.Elapsed.TotalMilliseconds -lt $deadlineMs) {
        $process.Refresh()
        if ($process.HasExited) { throw "Application exited during launch (exit $($process.ExitCode))" }
        if ($process.MainWindowHandle -ne [IntPtr]::Zero) {
          $result.windowVisibleMs = [Math]::Round($watch.Elapsed.TotalMilliseconds,1)
          break
        }
        Start-Sleep -Milliseconds 25
      }
      if ($null -eq $result.windowVisibleMs) { throw 'No top-level window before timeout' }
      while ($watch.Elapsed.TotalMilliseconds -lt $deadlineMs) {
        $process.Refresh()
        if ($process.HasExited) { throw 'Application exited before message-loop readiness' }
        if ($process.Responding) {
          $result.windowResponsiveMs = [Math]::Round($watch.Elapsed.TotalMilliseconds,1)
          break
        }
        Start-Sleep -Milliseconds 25
      }
      $cpuBefore = $process.TotalProcessorTime.TotalSeconds
      $ioBefore = Get-ProcessIo $process.Id
      Start-Sleep -Seconds $ObservationSeconds
      $process.Refresh()
      if ($process.HasExited) { throw 'Application exited during observation' }
      $result.cpuCoreSeconds = [Math]::Round($process.TotalProcessorTime.TotalSeconds - $cpuBefore,3)
      $result.workingSetMB = [Math]::Round($process.WorkingSet64 / 1MB,1)
      $result.privateMemoryMB = [Math]::Round($process.PrivateMemorySize64 / 1MB,1)
      $ioAfter = Get-ProcessIo $process.Id
      if ($ioBefore -and $ioAfter) {
        $result.readMB = [Math]::Round(($ioAfter.read - $ioBefore.read) / 1MB,2)
        $result.writeMB = [Math]::Round(($ioAfter.write - $ioBefore.write) / 1MB,2)
      }
    } catch {
      $result.error = $_.Exception.Message
    } finally {
      if ($null -ne $process) {
        try {
          if (-not $process.HasExited) {
            [void]$process.CloseMainWindow()
            if (-not $process.WaitForExit(2500)) {
              # Only the exact test-owned Process handle is terminated; no process tree.
              $process.Kill()
              [void]$process.WaitForExit(5000)
            }
          }
        } finally {
          $process.Dispose()
        }
      }
    }
    $observations.Add([pscustomobject]$result)
    Write-Output ("{0} round {1}: window={2}ms responsive={3}ms CPU={4}s RAM={5}MB {6}" -f $label,$round,$result.windowVisibleMs,$result.windowResponsiveMs,$result.cpuCoreSeconds,$result.workingSetMB,$result.error)
  }
}

$stats = foreach ($label in @('baseline','candidate')) {
  $good = @($observations | Where-Object { $_.variant -eq $label -and $null -eq $_.error })
  [pscustomobject]@{
    variant = $label
    successful = $good.Count
    attempted = $Runs
    medianWindowMs = Get-Median @($good | ForEach-Object { $_.windowVisibleMs })
    medianResponsiveMs = Get-Median @($good | Where-Object { $null -ne $_.windowResponsiveMs } | ForEach-Object { $_.windowResponsiveMs })
    medianWorkingSetMB = Get-Median @($good | ForEach-Object { $_.workingSetMB })
    medianCpuCoreSeconds = Get-Median @($good | ForEach-Object { $_.cpuCoreSeconds })
  }
}
$report = [ordered]@{
  schemaVersion = 1
  observedAtUtc = (Get-Date).ToUniversalTime().ToString('o')
  benchmark = 'windows-gpui-isolated-process-start'
  interpretation = 'WindowHandle and Process.Responding are proxies, not first paint or usable Editor/Agent readiness'
  sanitizedStateTemplateUsed = ($null -ne $srcState)
  cpuAndIoObservationSeconds = $ObservationSeconds
  binarySha256 = $hashes
  summary = @($stats)
  observations = @($observations)
}
$output = Join-Path $scratch ("startup-benchmark-{0}.json" -f $session)
$report | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $output -Encoding utf8
$stats | Format-Table -AutoSize
Write-Output ("RESULT_JSON=" + $output)
if (@($observations | Where-Object { $null -ne $_.error }).Count -gt 0) { exit 2 }
