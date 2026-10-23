param(
  [Parameter(Mandatory=$true)][ValidateSet('click','rightClick')][string]$Mode,
  [Parameter(Mandatory=$true)][ValidateRange(1,6000)][int]$X,
  [Parameter(Mandatory=$true)][ValidateRange(1,4000)][int]$Y,
  [Parameter(Mandatory=$true)][ValidateRange(1,2147483647)][int]$TargetPid,
  [ValidateSet('Debug','Release')][string]$Configuration='Debug'
)
$ErrorActionPreference='Stop'
$root=(Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$binary=if($Configuration -eq 'Release') {
  'build/windows/shared_release_ninja/runner/Release/alera-dev.exe'
} else {
  'build/windows/shared_gui_ninja/runner/Debug/alera-dev.exe'
}
$expectedExe=(Resolve-Path (Join-Path $root $binary)).Path
$app=Get-Process -Id $TargetPid -ErrorAction Stop
if($app.Path -ne $expectedExe -or $app.MainWindowHandle -eq [IntPtr]::Zero) {
  throw 'Refusing input: exact isolated E2E executable/HWND does not match'
}
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class AleraE2ERealMouse {
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X,Y; }
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left,Top,Right,Bottom; }
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h,out RECT r);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(POINT p);
  [DllImport("user32.dll")] public static extern IntPtr GetAncestor(IntPtr h,uint flags);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h,out uint id);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x,int y);
  [DllImport("user32.dll")] public static extern bool GetCursorPos(out POINT p);
  [DllImport("user32.dll")] public static extern void mouse_event(uint flags,uint dx,uint dy,uint data,UIntPtr extra);
}
'@
$hwnd=$app.MainWindowHandle
$rect=New-Object AleraE2ERealMouse+RECT
if(-not [AleraE2ERealMouse]::GetWindowRect($hwnd,[ref]$rect)) {
  throw 'Could not determine isolated E2E window bounds'
}
$width=$rect.Right-$rect.Left
$height=$rect.Bottom-$rect.Top
if($X -ge $width -or $Y -ge $height) {
  throw "Refusing input outside E2E main HWND: relative=$X,$Y size=${width}x$height"
}
$screenX=$rect.Left+$X
$screenY=$rect.Top+$Y
$point=New-Object AleraE2ERealMouse+POINT
$point.X=$screenX
$point.Y=$screenY
function Assert-E2EOwner([IntPtr]$candidate,[string]$stage) {
  if($candidate -eq [IntPtr]::Zero){throw "Refusing ${stage}: no native window"}
  $rootHwnd=[AleraE2ERealMouse]::GetAncestor($candidate,2)
  [uint32]$owner=0
  [void][AleraE2ERealMouse]::GetWindowThreadProcessId($rootHwnd,[ref]$owner)
  if($owner -ne [uint32]$TargetPid){
    throw "Refusing ${stage}: pointer belongs to another process"
  }
}
[void][AleraE2ERealMouse]::SetForegroundWindow($hwnd)
Start-Sleep -Milliseconds 120
Assert-E2EOwner ([AleraE2ERealMouse]::GetForegroundWindow()) 'foreground'
Assert-E2EOwner ([AleraE2ERealMouse]::WindowFromPoint($point)) 'hit test'
$original=New-Object AleraE2ERealMouse+POINT
$canRestore=[AleraE2ERealMouse]::GetCursorPos([ref]$original)
try {
  if(-not [AleraE2ERealMouse]::SetCursorPos($screenX,$screenY)){
    throw 'SetCursorPos failed'
  }
  Assert-E2EOwner ([AleraE2ERealMouse]::WindowFromPoint($point)) 'post-move hit test'
  if($Mode -eq 'rightClick'){
    [AleraE2ERealMouse]::mouse_event(0x8,0,0,0,[UIntPtr]::Zero)
    [AleraE2ERealMouse]::mouse_event(0x10,0,0,0,[UIntPtr]::Zero)
  }else{
    [AleraE2ERealMouse]::mouse_event(0x2,0,0,0,[UIntPtr]::Zero)
    [AleraE2ERealMouse]::mouse_event(0x4,0,0,0,[UIntPtr]::Zero)
  }
  Write-Output "E2E_REAL_MOUSE_POSTED=1 PID=$TargetPid MODE=$Mode CLIENT=$X,$Y"
}finally{
  if($canRestore){[void][AleraE2ERealMouse]::SetCursorPos($original.X,$original.Y)}
}
