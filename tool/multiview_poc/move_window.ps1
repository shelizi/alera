param(
  [Parameter(Mandatory=$true)][long]$Hwnd,
  [Parameter(Mandatory=$true)][int]$X,
  [Parameter(Mandatory=$true)][int]$Y,
  [int]$W = 0,
  [int]$H = 0
)
Add-Type @'
using System;using System.Runtime.InteropServices;
public static class Mover {
  [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr ins, int x, int y, int cx, int cy, uint f);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  public struct RECT { public int L,T,R,B; }
}
'@
[void][Mover]::SetThreadDpiAwarenessContext([IntPtr]::New(-4))
$r=New-Object Mover+RECT
[void][Mover]::GetWindowRect([IntPtr]$Hwnd,[ref]$r)
if($W -le 0){ $W = $r.R-$r.L }
if($H -le 0){ $H = $r.B-$r.T }
[void][Mover]::SetWindowPos([IntPtr]$Hwnd,[IntPtr]::Zero,$X,$Y,$W,$H,0x0010 -bor 0x0200)
[void][Mover]::GetWindowRect([IntPtr]$Hwnd,[ref]$r)
"MOVED hwnd=$Hwnd rect=$($r.L),$($r.T)-$($r.R),$($r.B)"
