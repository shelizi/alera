param(
  [Parameter(Mandatory=$true)][int]$FromX,
  [Parameter(Mandatory=$true)][int]$FromY,
  [Parameter(Mandatory=$true)][int]$ToX,
  [Parameter(Mandatory=$true)][int]$ToY,
  [int]$Steps = 20
)
$ErrorActionPreference='Stop'
Add-Type @'
using System;using System.Runtime.InteropServices;
public static class MvOnly {
  [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
}
'@
[void][MvOnly]::SetThreadDpiAwarenessContext([IntPtr]::New(-4))
for($i=0;$i -le $Steps;$i++){
  $x=[int]($FromX+($ToX-$FromX)*$i/$Steps)
  $y=[int]($FromY+($ToY-$FromY)*$i/$Steps)
  [void][MvOnly]::SetCursorPos($x,$y)
  Start-Sleep -Milliseconds 30
}
"MOVED_ONLY $FromX,$FromY -> $ToX,$ToY"
