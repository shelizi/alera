param([long]$Foreground = 0)
Add-Type @'
using System;using System.Runtime.InteropServices;
public class EK {
  [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint f, UIntPtr e);
}
'@
[void][EK]::SetThreadDpiAwarenessContext([IntPtr]::New(-4))
if ($Foreground -ne 0) { [void][EK]::SetForegroundWindow([IntPtr]$Foreground); Start-Sleep -Milliseconds 250 }
[EK]::keybd_event(0x1B,0,0,[UIntPtr]::Zero); Start-Sleep -Milliseconds 60
[EK]::keybd_event(0x1B,0,2,[UIntPtr]::Zero)
'ESC_SENT'
