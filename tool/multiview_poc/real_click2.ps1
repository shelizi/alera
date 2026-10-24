param(
  [Parameter(Mandatory=$true)][int]$X,
  [Parameter(Mandatory=$true)][int]$Y,
  [ValidateSet('left','right')][string]$Button = 'left',
  [long]$Foreground = 0
)
$ErrorActionPreference='Stop'
Add-Type @'
using System;using System.Runtime.InteropServices;
public class RC2 {
  [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern int GetSystemMetrics(int i);
  [DllImport("user32.dll")] public static extern uint SendInput(uint n, INPUT[] p, int cb);
  [StructLayout(LayoutKind.Sequential)] public struct MOUSEINPUT { public int dx,dy; public uint mouseData,dwFlags,time; public UIntPtr dwExtraInfo; }
  [StructLayout(LayoutKind.Sequential)] public struct INPUT { public uint type; public MOUSEINPUT mi; }
  public struct POINT { public int X, Y; }
  [DllImport("user32.dll")] public static extern bool GetCursorPos(out POINT p);
}
'@
[void][RC2]::SetThreadDpiAwarenessContext([IntPtr]::New(-4))
if ($Foreground -ne 0) {
  [void][RC2]::SetForegroundWindow([IntPtr]$Foreground)
  Start-Sleep -Milliseconds 300
}
$vL=[RC2]::GetSystemMetrics(76); $vT=[RC2]::GetSystemMetrics(77)
$vW=[RC2]::GetSystemMetrics(78); $vH=[RC2]::GetSystemMetrics(79)
$i1=New-Object RC2+INPUT; $i1.type=0
$i1.mi.dx=[int](($X-$vL)*65535/$vW); $i1.mi.dy=[int](($Y-$vT)*65535/$vH)
$i1.mi.dwFlags=0x8000 -bor 0x4000 -bor 0x0001
$i2=New-Object RC2+INPUT; $i2.type=0
$i3=New-Object RC2+INPUT; $i3.type=0
if($Button -eq 'right'){ $i2.mi.dwFlags=0x0008; $i3.mi.dwFlags=0x0010 }
else { $i2.mi.dwFlags=0x0002; $i3.mi.dwFlags=0x0004 }
$batch=[RC2+INPUT[]]@($i1,$i2,$i3)
$sent=[RC2]::SendInput(3,$batch,[Runtime.InteropServices.Marshal]::SizeOf([type][RC2+INPUT]))
$p=New-Object RC2+POINT
[void][RC2]::GetCursorPos([ref]$p)
"CLICK_SENT button=$Button phys=$X,$Y injected=$sent vdesk=$vL,$vT ${vW}x${vH} cursorAfter=$($p.X),$($p.Y)"
