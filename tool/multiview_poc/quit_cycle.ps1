param(
  [Parameter(Mandatory=$true)][long]$Hwnd,
  [Parameter(Mandatory=$true)][int]$TargetPid,
  [int]$WaitMs = 2500
)
$ErrorActionPreference='Stop'
Add-Type @'
using System;using System.Runtime.InteropServices;
public static class QuitCycle {
  [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h,uint m,UIntPtr w,IntPtr l);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h,out RECT r);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x,int y);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr h);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f,uint x,uint y,uint d,UIntPtr e);
  public struct RECT{public int Left,Top,Right,Bottom;}
}
'@
[void][QuitCycle]::SetThreadDpiAwarenessContext([IntPtr]::New(-4))
$r=New-Object QuitCycle+RECT
[void][QuitCycle]::GetWindowRect([IntPtr]$Hwnd,[ref]$r)
$w=$r.Right-$r.Left; $h=$r.Bottom-$r.Top
# Post the in-app Quit menu command (0x111 WM_COMMAND, 1004 = Quit).
[void][QuitCycle]::PostMessage([IntPtr]$Hwnd,0x111,[UIntPtr]::op_Explicit(1004),[IntPtr]::Zero)
Write-Output "QUIT_POSTED pid=$TargetPid hwnd=$Hwnd"
Start-Sleep -Milliseconds $WaitMs
[void][QuitCycle]::BringWindowToTop([IntPtr]$Hwnd)
[void][QuitCycle]::SetForegroundWindow([IntPtr]$Hwnd)
Start-Sleep -Milliseconds 400
# The busy-runtime confirmation dialog is centered; the destructive
# "Force Stop And Quit" button sits in the lower half of the dialog.
# Physical coords: dialog center ~= window center; button ~ +55px below
# center, dialog is ~440 logical * scale wide.
$cx=$r.Left + [int]($w*0.5)
$by=$r.Top + [int]($h*0.56)
[void][QuitCycle]::SetCursorPos($cx,$by)
Start-Sleep -Milliseconds 350
[QuitCycle]::mouse_event(0x0002,0,0,0,[UIntPtr]::Zero)
Start-Sleep -Milliseconds 120
[QuitCycle]::mouse_event(0x0004,0,0,0,[UIntPtr]::Zero)
Write-Output "FORCE_CLICKED phys=$cx,$by"
