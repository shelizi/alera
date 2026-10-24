param(
  [Parameter(Mandatory=$true)][int]$FromX,
  [Parameter(Mandatory=$true)][int]$FromY,
  [Parameter(Mandatory=$true)][int]$ToX,
  [Parameter(Mandatory=$true)][int]$ToY,
  [long]$Foreground = 0,
  [int]$Steps = 30,
  [int]$StepMs = 25,
  [int]$HoldMs = 0
)
$ErrorActionPreference='Stop'
Add-Type @'
using System;using System.Runtime.InteropServices;
public class RD2 {
  [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern int GetSystemMetrics(int i);
  [DllImport("user32.dll")] public static extern uint SendInput(uint n, INPUT[] p, int cb);
  [DllImport("user32.dll")] public static extern bool GetCursorPos(out POINT p);
  [StructLayout(LayoutKind.Sequential)] public struct MOUSEINPUT { public int dx,dy; public uint mouseData,dwFlags,time; public UIntPtr dwExtraInfo; }
  [StructLayout(LayoutKind.Sequential)] public struct INPUT { public uint type; public MOUSEINPUT mi; }
  public struct POINT { public int X, Y; }
}
'@
[void][RD2]::SetThreadDpiAwarenessContext([IntPtr]::New(-4))
if ($Foreground -ne 0) {
  [void][RD2]::SetForegroundWindow([IntPtr]$Foreground)
  Start-Sleep -Milliseconds 300
}
$vL=[RD2]::GetSystemMetrics(76); $vT=[RD2]::GetSystemMetrics(77)
$vW=[RD2]::GetSystemMetrics(78); $vH=[RD2]::GetSystemMetrics(79)
function MkInput([double]$px,[double]$py,[uint32]$flags){
  $i=New-Object RD2+INPUT; $i.type=0
  if($flags -band 0x8000){
    $i.mi.dx=[int](($px-$vL)*65535/$vW); $i.mi.dy=[int](($py-$vT)*65535/$vH)
  }
  $i.mi.dwFlags=$flags
  return $i
}
$moveAbs=0x8000 -bor 0x4000 -bor 0x0001
[void][RD2]::SendInput(1,[RD2+INPUT[]]@(MkInput $FromX $FromY $moveAbs),[Runtime.InteropServices.Marshal]::SizeOf([type][RD2+INPUT]))
Start-Sleep -Milliseconds 250
[void][RD2]::SendInput(1,[RD2+INPUT[]]@(MkInput $FromX $FromY 0x0002),[Runtime.InteropServices.Marshal]::SizeOf([type][RD2+INPUT]))
Start-Sleep -Milliseconds 150
for($i=1; $i -le $Steps; $i++){
  $cx=$FromX + ($ToX-$FromX)*$i/$Steps
  $cy=$FromY + ($ToY-$FromY)*$i/$Steps
  [void][RD2]::SendInput(1,[RD2+INPUT[]]@(MkInput $cx $cy $moveAbs),[Runtime.InteropServices.Marshal]::SizeOf([type][RD2+INPUT]))
  Start-Sleep -Milliseconds $StepMs
}
if($HoldMs -gt 0){ Start-Sleep -Milliseconds $HoldMs }
[void][RD2]::SendInput(1,[RD2+INPUT[]]@(MkInput $ToX $ToY 0x0004),[Runtime.InteropServices.Marshal]::SizeOf([type][RD2+INPUT]))
$p=New-Object RD2+POINT
[void][RD2]::GetCursorPos([ref]$p)
"DRAG2_DONE from=$FromX,$FromY to=$ToX,$ToY cursorAfter=$($p.X),$($p.Y)"
