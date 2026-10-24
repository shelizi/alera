param(
  [Parameter(Mandatory=$true)][long]$Hwnd,
  [Parameter(Mandatory=$true)][string]$Text,
  [switch]$NoEnter
)
$ErrorActionPreference='Stop'
Add-Type @'
using System;using System.Runtime.InteropServices;
public class PC {
  [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr h,uint m,IntPtr w,IntPtr l);
}
'@
[void][PC]::SetThreadDpiAwarenessContext([IntPtr]::New(-4))
[void][PC]::SetForegroundWindow([IntPtr]$Hwnd)
Start-Sleep -Milliseconds 250
foreach($ch in $Text.ToCharArray()){
  [void][PC]::PostMessageW([IntPtr]$Hwnd,0x0102,[IntPtr][int][char]$ch,[IntPtr]1)
  Start-Sleep -Milliseconds 25
}
if(-not $NoEnter){ [void][PC]::PostMessageW([IntPtr]$Hwnd,0x0102,[IntPtr]13,[IntPtr]1) }
Write-Output "POSTED $($Text.Length) chars"
