param(
 [int]$TargetPid=0,
 [ValidateSet('Debug','Release')][string]$Configuration='Debug'
)
$ErrorActionPreference='Stop'
$root=(Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$buildPath = if($Configuration -eq 'Release') {
 'build/windows/shared_release_ninja/runner/Release/alera-dev.exe'
} else { 'build/windows/shared_gui_ninja/runner/Debug/alera-dev.exe' }
$expectedExe=Join-Path $root $buildPath
$ps=@(Get-Process alera-dev -ErrorAction SilentlyContinue | Where-Object {$_.Path -eq $expectedExe -and ($TargetPid -eq 0 -or $_.Id -eq $TargetPid)})
if($ps.Count -ne 1){throw "Exactly one E2E Alera required, found $($ps.Count)"}
$p=$ps[0]
Add-Type @'
using System;
using System.Collections.Generic;
using System.Text;
using System.Runtime.InteropServices;
public static class AleraE2ETopLevelWindows {
 public delegate bool WindowCallback(IntPtr hwnd,IntPtr lp);
 [DllImport("user32.dll")] public static extern bool EnumWindows(WindowCallback cb,IntPtr user);
 [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd,out uint pid);
 [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hwnd,StringBuilder text,int max);
 [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr hwnd);
 public static string[] GetWindows(int processId) {
   var list=new List<string>();
   EnumWindows((hwnd,unused)=>{
      uint actual;GetWindowThreadProcessId(hwnd,out actual);
      if(actual!=(uint)processId || !IsWindowVisible(hwnd))return true;
      var sb=new StringBuilder(256);
      GetWindowText(hwnd,sb,sb.Capacity);
      list.Add("HWND="+hwnd.ToInt64()+" TITLE="+sb+" DPI="+GetDpiForWindow(hwnd));
      return true;
   },IntPtr.Zero);
   return list.ToArray();
 }
}
'@
$wins=[AleraE2ETopLevelWindows]::GetWindows($p.Id)
Write-Output "PID=$($p.Id) RESPONDING=$($p.Responding) WINDOWS=$($wins.Count) RSS_MB=$([math]::Round($p.WorkingSet64/1MB,1)) HANDLE_COUNT=$($p.HandleCount)"
foreach($win in $wins){Write-Output $win}
