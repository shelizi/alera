param(
  [Parameter(Mandatory=$true)][ValidateRange(1,2147483647)][int]$TargetPid
)
$ErrorActionPreference='Stop'
Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Text;
using System.Collections.Generic;
public static class AleraWinList {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr l);
  [DllImport("user32.dll")] public static extern int GetWindowTextW(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint id);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  public static List<string> Rows = new List<string>();
  public static uint Wanted;
}
'@
[AleraWinList]::Wanted=[uint32]$TargetPid
$cb=[AleraWinList+EnumProc]{ param($h,$l)
  [uint32]$owner=0
  [void][AleraWinList]::GetWindowThreadProcessId($h,[ref]$owner)
  if($owner -eq [AleraWinList]::Wanted){
    $sb=New-Object System.Text.StringBuilder 256
    [void][AleraWinList]::GetWindowTextW($h,$sb,256)
    [AleraWinList]::Rows.Add(("{0} visible={1} title='{2}'" -f $h,[AleraWinList]::IsWindowVisible($h),$sb.ToString()))
  }
  return $true }
[void][AleraWinList]::EnumWindows($cb,[IntPtr]::Zero)
[AleraWinList]::Rows | ForEach-Object { Write-Output $_ }
