param([long]$Hwnd)
Add-Type @'
using System;using System.Runtime.InteropServices;using System.Text;
public class HI {
  [DllImport("user32.dll")] public static extern int GetClassNameW(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern int GetWindowTextW(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern IntPtr GetAncestor(IntPtr h, uint f);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  public struct RECT { public int L, T, R, B; }
}
'@
$root=[HI]::GetAncestor([IntPtr]$Hwnd,2)
$cn=New-Object Text.StringBuilder 256;[void][HI]::GetClassNameW([IntPtr]$Hwnd,$cn,256)
$wt=New-Object Text.StringBuilder 256;[void][HI]::GetWindowTextW([IntPtr]$Hwnd,$wt,256)
$rc=New-Object HI+RECT;[void][HI]::GetWindowRect([IntPtr]$Hwnd,[ref]$rc)
$pid=0;[void][HI]::GetWindowThreadProcessId([IntPtr]$Hwnd,[ref]$pid)
$rcn=New-Object Text.StringBuilder 256;[void][HI]::GetClassNameW($root,$rcn,256)
"hwnd=$Hwnd class=$($cn.ToString()) title=$($wt.ToString()) pid=$pid root=$root rootclass=$($rcn.ToString()) rect=$($rc.L),$($rc.T)-$($rc.R),$($rc.B)"
