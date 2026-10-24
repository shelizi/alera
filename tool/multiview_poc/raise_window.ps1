param([Parameter(Mandatory=$true)][long]$Hwnd)
Add-Type @'
using System;using System.Runtime.InteropServices;
public static class RW {
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h,IntPtr ins,int x,int y,int w,int ht,uint f);
  [DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr h);
}
'@
# HWND_TOP=0, SWP_NOMOVE|SWP_NOSIZE|SWP_SHOWWINDOW = 0x3|0x40
[void][RW]::SetWindowPos([IntPtr]$Hwnd,[IntPtr]::Zero,0,0,0,0,0x43)
[void][RW]::BringWindowToTop([IntPtr]$Hwnd)
Write-Output "RAISED hwnd=$Hwnd"
