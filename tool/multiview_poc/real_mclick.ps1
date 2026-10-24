param([int]$X,[int]$Y,[long]$Foreground=0)
Add-Type @'
using System;using System.Runtime.InteropServices;
public class MC {
  [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x,int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f,uint dx,uint dy,uint d,UIntPtr e);
}
'@
[void][MC]::SetThreadDpiAwarenessContext([IntPtr]::New(-4))
if($Foreground -ne 0){[void][MC]::SetForegroundWindow([IntPtr]$Foreground);Start-Sleep -Milliseconds 250}
[void][MC]::SetCursorPos($X,$Y);Start-Sleep -Milliseconds 250
[MC]::mouse_event(0x0020,0,0,0,[UIntPtr]::Zero);Start-Sleep -Milliseconds 60
[MC]::mouse_event(0x0040,0,0,0,[UIntPtr]::Zero)
"MCLICK_DONE $X,$Y"
