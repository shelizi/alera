param(
  [Parameter(Mandatory=$true)][long]$Hwnd,
  [Parameter(Mandatory=$true)][int]$ScreenX,
  [Parameter(Mandatory=$true)][int]$ScreenY
)
$ErrorActionPreference='Stop'
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class AleraCV {
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern bool GetCursorPos(out POINT p);
  [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(POINT p);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint dx, uint dy, uint d, UIntPtr e);
  [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
  public struct POINT { public int X, Y; }
}
'@
$prev=[AleraCV]::SetThreadDpiAwarenessContext([IntPtr]::New(-4))
try {
  [void][AleraCV]::SetForegroundWindow([IntPtr]$Hwnd)
  Start-Sleep -Milliseconds 300
  [void][AleraCV]::SetCursorPos($ScreenX,$ScreenY)
  Start-Sleep -Milliseconds 200
  $pos=New-Object AleraCV+POINT
  [void][AleraCV]::GetCursorPos([ref]$pos)
  $under=[AleraCV]::WindowFromPoint($pos)
  Write-Output "cursor=$($pos.X),$($pos.Y) under=$under fg=$([AleraCV]::GetForegroundWindow())"
  Start-Sleep -Milliseconds 150
  [AleraCV]::mouse_event(0x0002,0,0,0,[UIntPtr]::Zero)
  Start-Sleep -Milliseconds 90
  [AleraCV]::mouse_event(0x0004,0,0,0,[UIntPtr]::Zero)
  Write-Output 'CLICK_DONE'
} finally { [void][AleraCV]::SetThreadDpiAwarenessContext($prev) }
