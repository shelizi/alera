param([Parameter(Mandatory=$true)][long]$Hwnd)
Add-Type @'
using System;using System.Runtime.InteropServices;
public static class SW6 {
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h,int c);
}
'@
[void][SW6]::ShowWindow([IntPtr]$Hwnd, 6)
Write-Output "MINIMIZED hwnd=$Hwnd"
