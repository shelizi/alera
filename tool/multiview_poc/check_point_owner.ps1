param(
  [Parameter(Mandatory=$true)][long]$Hwnd,
  [Parameter(Mandatory=$true)][int]$X,
  [Parameter(Mandatory=$true)][int]$Y
)
$ErrorActionPreference='Stop'
Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class AleraPointCheck {
  [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(POINT p);
  [DllImport("user32.dll")] public static extern IntPtr GetAncestor(IntPtr h, uint f);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint id);
  [DllImport("user32.dll")] public static extern int GetWindowTextW(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
  public struct POINT { public int X, Y; }
  public struct RECT { public int Left, Top, Right, Bottom; }
}
'@
[void][AleraPointCheck]::SetThreadDpiAwarenessContext([IntPtr]::New(-4))
$r=New-Object AleraPointCheck+RECT
[void][AleraPointCheck]::GetWindowRect([IntPtr]$Hwnd,[ref]$r)
Write-Output ("childRect L={0} T={1} R={2} B={3}" -f $r.Left,$r.Top,$r.Right,$r.Bottom)
$pt=New-Object AleraPointCheck+POINT; $pt.X=$X; $pt.Y=$Y
$hit=[AleraPointCheck]::WindowFromPoint($pt)
$root=[AleraPointCheck]::GetAncestor($hit,2)
$sb=New-Object System.Text.StringBuilder 256
[void][AleraPointCheck]::GetWindowTextW($root,$sb,256)
[uint32]$owner=0
[void][AleraPointCheck]::GetWindowThreadProcessId($root,[ref]$owner)
Write-Output ("atPoint hit={0} root={1} pid={2} title='{3}'" -f $hit,$root,$owner,$sb.ToString())
Write-Output ("fgBefore={0}" -f [AleraPointCheck]::GetForegroundWindow())
[void][AleraPointCheck]::SetForegroundWindow([IntPtr]$Hwnd)
Start-Sleep -Milliseconds 400
Write-Output ("fgAfter={0}" -f [AleraPointCheck]::GetForegroundWindow())
