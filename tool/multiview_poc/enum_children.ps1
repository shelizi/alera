param(
  [Parameter(Mandatory=$true)][long]$Hwnd
)
Add-Type @'
using System;using System.Runtime.InteropServices;using System.Text;
public static class EnumKids {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr p, EnumProc cb, IntPtr l);
  [DllImport("user32.dll")] public static extern int GetClassNameW(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  public struct RECT { public int L,T,R,B; }
}
'@
$v=[IntPtr]$Hwnd
$found = New-Object System.Collections.Generic.List[string]
[EnumKids]::EnumChildWindows($v,[EnumKids+EnumProc]{ param($h,$l)
  $cn=New-Object Text.StringBuilder 128
  [void][EnumKids]::GetClassNameW($h,$cn,128)
  $r=New-Object EnumKids+RECT
  [void][EnumKids]::GetWindowRect($h,[ref]$r)
  $found.Add("child=$h class=$($cn.ToString()) rect=$($r.L),$($r.T)-$($r.R),$($r.B)")
  return $true
},[IntPtr]::Zero) | Out-Null
$found | ForEach-Object { Write-Output $_ }
