param([ValidateSet('Debug','Release')][string]$Configuration='Debug')

$ErrorActionPreference = 'Stop'
$dir = Join-Path $PSScriptRoot "build/windows/poc_ninja/runner/$Configuration"
$exe = Join-Path $dir 'alera_multiview_poc.exe'
$ready = Join-Path $dir 'poc_handoff_waiting_wm_close.json'
$report = Join-Path $dir 'poc_handoff_report.json'
foreach ($file in @($ready, $report)) {
  if (Test-Path -LiteralPath $file) { Remove-Item -LiteralPath $file -Force }
}
Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class AleraExternalNativeClose {
  delegate bool Callback(IntPtr hwnd, IntPtr ptr);
  [DllImport("user32.dll")] static extern bool EnumWindows(Callback cb,IntPtr user);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr hwnd,out uint pid);
  [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetWindowText(IntPtr hwnd,StringBuilder dst,int max);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr hwnd);
  [DllImport("user32.dll")] static extern bool PostMessage(IntPtr hwnd,uint msg,IntPtr wp,IntPtr lp);

  public static IntPtr Find(int processId,string title) {
    IntPtr found=IntPtr.Zero;
    EnumWindows((hwnd, unused) => {
      uint pid;
      GetWindowThreadProcessId(hwnd,out pid);
      if(pid!=(uint)processId || !IsWindowVisible(hwnd)) return true;
      var sb=new StringBuilder(256);
      GetWindowText(hwnd,sb,sb.Capacity);
      if(sb.ToString()!=title) return true;
      found=hwnd;
      return false;
    },IntPtr.Zero);
    return found;
  }
  public static bool Close(int processId,string title,out long handle) {
    var hwnd=Find(processId,title);
    handle=hwnd.ToInt64();
    return hwnd!=IntPtr.Zero &&
      PostMessage(hwnd,0x0010,IntPtr.Zero,IntPtr.Zero);
  }
}
'@

Set-Location -LiteralPath $dir
$app = Start-Process -FilePath $exe -ArgumentList @('--poc-handoff','--poc-handoff-wmclose') -WorkingDirectory $dir -PassThru
try {
  Write-Output "START pid=$($app.Id) config=$Configuration"
  for ($i=0; $i -lt 80; $i++) {
    if (Test-Path -LiteralPath $ready) { break }
    $app.Refresh()
    if ($app.HasExited) { throw "PoC terminated before external close" }
    Start-Sleep -Milliseconds 250
  }
  if (-not (Test-Path -LiteralPath $ready)) { throw "Native close target timeout" }
  $target = Get-Content -LiteralPath $ready -Raw | ConvertFrom-Json
  if ([int]$target.pid -ne [int]$app.Id) { throw 'PID mismatch in test report' }
  $handle = [long]0
  if (-not [AleraExternalNativeClose]::Close($app.Id, [string]$target.targetWindowTitle, [ref]$handle)) {
    throw 'Could not post Win32 WM_CLOSE to exact PoC-owned child HWND'
  }
  Write-Output "POSTED_WM_CLOSE pid=$($app.Id) hwnd=$handle title=$($target.targetWindowTitle)"
  for ($i=0; $i -lt 70; $i++) {
    if (Test-Path $report) { break }
    $app.Refresh()
    if ($app.HasExited) { break }
    Start-Sleep -Milliseconds 200
  }
  if (-not (Test-Path $report)) { throw 'No handoff report after Win32 close' }
  $result = Get-Content $report -Raw | ConvertFrom-Json
  Write-Output "RESULT passed=$($result.passed) externalWmCloseObserved=$($result.externalWmCloseObserved) observerEvents=$($result.nativeCloseEvents) autorecovery=$($result.automaticRecoveries) cycles=$($result.handoffTimings.Count)"
  if ($result.error) { Write-Output "ERROR=$($result.error)" }
  if (-not $result.passed -or -not $result.externalWmCloseObserved) { exit 4 }
} finally {
  $app.Refresh()
  if (-not $app.HasExited) {
    if (-not $app.WaitForExit(5000)) { Stop-Process -Id $app.Id -Force }
  }
}
