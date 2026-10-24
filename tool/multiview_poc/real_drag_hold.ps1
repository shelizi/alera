param(
  [Parameter(Mandatory=$true)][int]$FromX,
  [Parameter(Mandatory=$true)][int]$FromY,
  [Parameter(Mandatory=$true)][int]$ToX,
  [Parameter(Mandatory=$true)][int]$ToY,
  [long]$Foreground = 0,
  [int]$Steps = 25,
  [int]$StepMs = 16,
  [int]$HoldMs = 3000
)
$ErrorActionPreference='Stop'
Add-Type @'
using System;using System.Runtime.InteropServices;
public class RDH {
  [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint dx, uint dy, uint d, UIntPtr e);
}
'@
[void][RDH]::SetThreadDpiAwarenessContext([IntPtr]::New(-4))
if ($Foreground -ne 0) {
  [void][RDH]::SetForegroundWindow([IntPtr]$Foreground)
  Start-Sleep -Milliseconds 300
}
[void][RDH]::SetCursorPos($FromX, $FromY)
Start-Sleep -Milliseconds 250
[RDH]::mouse_event(0x0002,0,0,0,[UIntPtr]::Zero)
Start-Sleep -Milliseconds 120
for($i=1; $i -le $Steps; $i++){
  $cx=$FromX + [int](($ToX-$FromX)*$i/$Steps)
  $cy=$FromY + [int](($ToY-$FromY)*$i/$Steps)
  [void][RDH]::SetCursorPos($cx,$cy)
  [RDH]::mouse_event(0x0001,0,0,0,[UIntPtr]::Zero)
  Start-Sleep -Milliseconds $StepMs
}
"DRAG_HELD at=$ToX,$ToY holdMs=$HoldMs"
Start-Sleep -Milliseconds $HoldMs
[RDH]::mouse_event(0x0004,0,0,0,[UIntPtr]::Zero)
"DRAG_RELEASED"
