param(
  [Parameter(Mandatory=$true)][int]$FromX,
  [Parameter(Mandatory=$true)][int]$FromY,
  [Parameter(Mandatory=$true)][int]$ToX,
  [Parameter(Mandatory=$true)][int]$ToY,
  [long]$Foreground = 0,
  [int]$Steps = 25,
  [int]$StepMs = 16
)
$ErrorActionPreference='Stop'
Add-Type @'
using System;using System.Runtime.InteropServices;
public class RD {
  [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern bool GetCursorPos(out POINT p);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint dx, uint dy, uint d, UIntPtr e);
  public struct POINT { public int X, Y; }
}
'@
[void][RD]::SetThreadDpiAwarenessContext([IntPtr]::New(-4))
if ($Foreground -ne 0) {
  [void][RD]::SetForegroundWindow([IntPtr]$Foreground)
  Start-Sleep -Milliseconds 300
}
[void][RD]::SetCursorPos($FromX, $FromY)
Start-Sleep -Milliseconds 250
[RD]::mouse_event(0x0002,0,0,0,[UIntPtr]::Zero)   # LEFTDOWN
Start-Sleep -Milliseconds 120
for($i=1; $i -le $Steps; $i++){
  $cx=$FromX + [int](($ToX-$FromX)*$i/$Steps)
  $cy=$FromY + [int](($ToY-$FromY)*$i/$Steps)
  [void][RD]::SetCursorPos($cx,$cy)
  [RD]::mouse_event(0x0001,0,0,0,[UIntPtr]::Zero) # MOVE
  Start-Sleep -Milliseconds $StepMs
}
Start-Sleep -Milliseconds 120
[RD]::mouse_event(0x0004,0,0,0,[UIntPtr]::Zero)   # LEFTUP
$p=New-Object RD+POINT
[void][RD]::GetCursorPos([ref]$p)
"DRAG_DONE from=$FromX,$FromY to=$ToX,$ToY cursorAfter=$($p.X),$($p.Y)"
