param(
  [Parameter(Mandatory=$true)][long]$Hwnd,
  [Parameter(Mandatory=$true)][string]$Text,
  [switch]$NoEnter,
  [int]$GapMs = 20
)
$ErrorActionPreference='Stop'
Add-Type @'
using System;using System.Runtime.InteropServices;
public static class WC {
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr h,uint m,IntPtr w,IntPtr l);
  [DllImport("user32.dll")] public static extern IntPtr GetFocus();
}
'@
[void][WC]::SetForegroundWindow([IntPtr]$Hwnd)
Start-Sleep -Milliseconds 300
foreach($ch in $Text.ToCharArray()){
  # WM_KEYDOWN + WM_CHAR + WM_KEYUP sequence mimics real key delivery
  [void][WC]::PostMessageW([IntPtr]$Hwnd,0x0100,[IntPtr][int][char]$ch,[IntPtr]0)
  [void][WC]::PostMessageW([IntPtr]$Hwnd,0x0102,[IntPtr][int][char]$ch,[IntPtr]0)
  [void][WC]::PostMessageW([IntPtr]$Hwnd,0x0101,[IntPtr][int][char]$ch,[IntPtr]0)
  Start-Sleep -Milliseconds $GapMs
}
if(-not $NoEnter){
  [void][WC]::PostMessageW([IntPtr]$Hwnd,0x0100,[IntPtr]0x0D,[IntPtr]0)
  [void][WC]::PostMessageW([IntPtr]$Hwnd,0x0102,[IntPtr]0x0D,[IntPtr]0)
  [void][WC]::PostMessageW([IntPtr]$Hwnd,0x0101,[IntPtr]0x0D,[IntPtr]0)
}
Write-Output "WMCHAR_SENT len=$($Text.Length)"
