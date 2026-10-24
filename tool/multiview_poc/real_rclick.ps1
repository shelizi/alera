param(
  [Parameter(Mandatory=$true)][int]$X,
  [Parameter(Mandatory=$true)][int]$Y,
  [long]$Foreground = 0
)
$ErrorActionPreference='Stop'
Add-Type @'
using System;using System.Runtime.InteropServices;
public class RRC {
  [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern bool GetCursorPos(out POINT p);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint dx, uint dy, uint d, UIntPtr e);
  public struct POINT { public int X, Y; }
}
'@
[void][RRC]::SetThreadDpiAwarenessContext([IntPtr]::New(-4))
if ($Foreground -ne 0) {
  [void][RRC]::SetForegroundWindow([IntPtr]$Foreground)
  Start-Sleep -Milliseconds 300
}
[void][RRC]::SetCursorPos($X, $Y)
Start-Sleep -Milliseconds 250
[RRC]::mouse_event(0x0008,0,0,0,[UIntPtr]::Zero)   # RIGHTDOWN
Start-Sleep -Milliseconds 80
[RRC]::mouse_event(0x0010,0,0,0,[UIntPtr]::Zero)   # RIGHTUP
$p=New-Object RRC+POINT
[void][RRC]::GetCursorPos([ref]$p)
"RCLICK_DONE phys=$X,$Y cursorAfter=$($p.X),$($p.Y)"
