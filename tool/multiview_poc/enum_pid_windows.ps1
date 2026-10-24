param(
  [Parameter(Mandatory=$true)][int]$TargetPid
)
Add-Type @'
using System;using System.Runtime.InteropServices;using System.Text;
public static class EnumAllW {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr l);
  [DllImport("user32.dll")] public static extern int GetClassNameW(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern int GetWindowTextW(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint id);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
}
'@
$found = New-Object System.Collections.Generic.List[string]
[EnumAllW]::EnumWindows([EnumAllW+EnumProc]{ param($h,$l)
  [uint32]$wpid=0
  [void][EnumAllW]::GetWindowThreadProcessId($h,[ref]$wpid)
  if($wpid -eq [uint32]$TargetPid){
    $cn=New-Object Text.StringBuilder 128
    [void][EnumAllW]::GetClassNameW($h,$cn,128)
    $t=New-Object Text.StringBuilder 256
    [void][EnumAllW]::GetWindowTextW($h,$t,256)
    $found.Add("hwnd=$h class=$($cn.ToString()) vis=$([EnumAllW]::IsWindowVisible($h)) title=$($t.ToString())")
  }
  return $true
},[IntPtr]::Zero) | Out-Null
$found | ForEach-Object { Write-Output $_ }
