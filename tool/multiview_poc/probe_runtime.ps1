param(
  [ValidateSet("Debug", "Release")][string]$Configuration = "Debug",
  [ValidateSet("short", "stress", "long", "handoff", "handoff-stress")][string]$Mode = "long",
  [int]$TimeoutSeconds = 220
)

$ErrorActionPreference = "Stop"
$dir = Join-Path $PSScriptRoot "build/windows/poc_ninja/runner/$Configuration"
$exe = Join-Path $dir "alera_multiview_poc.exe"
if (-not (Test-Path -LiteralPath $exe)) { throw "Build does not exist: $exe" }
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Text;
using System.Runtime.InteropServices;

public static class AleraWindowProbe {
  private delegate bool WindowCallback(IntPtr hwnd, IntPtr lparam);
  [DllImport("user32.dll")] private static extern bool EnumWindows(WindowCallback cb, IntPtr lp);
  [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
  [DllImport("user32.dll")] private static extern bool IsWindowVisible(IntPtr hwnd);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] private static extern int GetWindowText(IntPtr hwnd, StringBuilder text, int max);
  [DllImport("user32.dll")] private static extern uint GetDpiForWindow(IntPtr hwnd);
  [DllImport("user32.dll")] public static extern uint GetGuiResources(IntPtr process, uint flags);

  public static string[] WindowsFor(int pid) {
    var names = new List<string>();
    EnumWindows((hwnd, _) => {
      uint actual;
      GetWindowThreadProcessId(hwnd, out actual);
      if (actual != (uint)pid || !IsWindowVisible(hwnd)) return true;
      var text = new StringBuilder(256);
      GetWindowText(hwnd, text, text.Capacity);
      names.Add(string.Format("{0}:{1}:dpi={2}", hwnd.ToInt64(), text.ToString(), GetDpiForWindow(hwnd)));
      return true;
    }, IntPtr.Zero);
    return names.ToArray();
  }
}
'@
Set-Location -LiteralPath $dir
$handoff = $Mode.StartsWith("handoff")
$report = Join-Path $dir $(if ($handoff) { "poc_handoff_report.json" } else { "poc_multiview_results.json" })
$samples = Join-Path $dir $(if ($handoff) { "poc_handoff_process_samples.csv" } else { "poc_process_samples.csv" })
foreach ($p in @($report, $samples, (Join-Path $dir "poc_startup_trace.txt"))) {
  if (Test-Path $p) { Remove-Item -LiteralPath $p -Force }
}
[string[]]$launchArgs = if ($handoff) { @("--poc-handoff") } else { @("--poc-smoke") }
if ($Mode -eq "stress") { $launchArgs += "--poc-stress" }
if ($Mode -eq "long") { $launchArgs += "--poc-long" }
if ($Mode -eq "handoff-stress") { $launchArgs += "--poc-handoff-stress" }
$process = Start-Process -FilePath $exe -ArgumentList $launchArgs -WorkingDirectory $dir -PassThru
Write-Output "MONITOR start config=$Configuration mode=$Mode pid=$($process.Id)"
$records = New-Object System.Collections.Generic.List[object]
$sw = [System.Diagnostics.Stopwatch]::StartNew()
do {
  $proc = Get-Process -Id $process.Id -ErrorAction SilentlyContinue
  if ($null -ne $proc) {
    $wins = [AleraWindowProbe]::WindowsFor($process.Id)
    $records.Add([pscustomobject]@{
      elapsedMs = $sw.ElapsedMilliseconds
      pid = $proc.Id
      workingMiB = [math]::Round($proc.WorkingSet64 / 1MB, 3)
      privateMiB = [math]::Round($proc.PrivateMemorySize64 / 1MB, 3)
      handles = $proc.HandleCount
      gdi = [AleraWindowProbe]::GetGuiResources($proc.Handle, 0)
      user = [AleraWindowProbe]::GetGuiResources($proc.Handle, 1)
      visibleNativeWindows = $wins.Count
      nativeWindows = $wins -join ";"
    })
  }
  $process.Refresh()
  if ($process.HasExited) { break }
  Start-Sleep -Milliseconds 500
} while ($sw.Elapsed.TotalSeconds -lt $TimeoutSeconds)
$process.Refresh()
if (-not $process.HasExited) {
  Stop-Process -Id $process.Id -Force
  Write-Output "MONITOR timeout killed_pid=$($process.Id)"
}
$records | Export-Csv -LiteralPath $samples -NoTypeInformation -Encoding utf8
if (-not (Test-Path -LiteralPath $report)) {
  Write-Output "MONITOR NO_REPORT exit=$($process.ExitCode)"
  $trace = Join-Path $dir "poc_startup_trace.txt"
  if (Test-Path $trace) { Get-Content $trace -Tail 8 }
  exit 2
}
$j = Get-Content -LiteralPath $report -Raw | ConvertFrom-Json
$create = if ($handoff) {
  @($j.handoffTimings | ForEach-Object { [double]$_.nativeWindowCreatedMs } | Sort-Object)
} else {
  @($j.cycles | ForEach-Object { [double]$_.createMs } | Sort-Object)
}
$frame = if ($handoff) {
  @($j.handoffTimings | ForEach-Object { [double]$_.editorMountedInChildMs } | Sort-Object)
} else {
  @($j.cycles | ForEach-Object { [double]$_.dartFirstFrameCallbackMs } | Sort-Object)
}
$close = if ($handoff) {
  @($j.handoffTimings | ForEach-Object { [double]$_.editorMountedBackInMainMs } | Sort-Object)
} else {
  @($j.cycles | ForEach-Object { [double]$_.closeMs } | Sort-Object)
}
$half = [int][math]::Floor(($create.Count - 1) / 2)
$p95 = [int][math]::Ceiling($create.Count * 0.95) - 1
if ($create.Count -gt 0) {
  Write-Output ("CREATE n={0} median={1:F1} p95={2:F1} max={3:F1} ms" -f $create.Count, $create[$half], $create[$p95], $create[-1])
  $frameLabel = if ($handoff) { "EDITOR_MOUNT_CHILD" } else { "FRAME_CALLBACK" }
  $closeLabel = if ($handoff) { "EDITOR_MOUNT_MAIN" } else { "CLOSE" }
  Write-Output ("{0} n={1} median={2:F1} p95={3:F1} max={4:F1} ms" -f $frameLabel, $frame.Count, $frame[$half], $frame[$p95], $frame[-1])
  Write-Output ("{0} n={1} median={2:F1} p95={3:F1} max={4:F1} ms" -f $closeLabel, $close.Count, $close[$half], $close[$p95], $close[-1])
}
if ($records.Count -gt 0) {
  $first = $records[0]
  $last = $records[-1]
  $peak = $records | Measure-Object workingMiB -Maximum
  $peakNative = $records | Measure-Object visibleNativeWindows -Maximum
  Write-Output ("PROCESS samples={0} rss_first={1} rss_last={2} rss_peak={3} handles_first={4} handles_last={5} gdi_first={6} gdi_last={7} user_first={8} user_last={9} visible_native_peak={10}" -f $records.Count,$first.workingMiB,$last.workingMiB,$peak.Maximum,$first.handles,$last.handles,$first.gdi,$last.gdi,$first.user,$last.user,$peakNative.Maximum)
}
if ($handoff) {
  Write-Output "RESULT passed=$($j.passed) cycles=$($j.handoffTimings.Count) observerEvents=$($j.nativeCloseEvents) terminalHostStartCount=$($j.terminalHostStartCount) processId=$($j.pid) error=$($j.error)"
} else {
  Write-Output "RESULT passed=$($j.passed) cycles=$($j.cycles.Count) finalViews=$($j.finalViews) parallelViews=$($j.parallelTest.peakViews) processId=$($j.processId) error=$($j.error)"
}
if (-not $j.passed) { exit 3 }
exit 0
