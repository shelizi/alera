param(
  [string]$OutputName = 'alera-shared-gui-window.png',
  [int]$TargetPid = 0,
  [string]$WindowTitle = '',
  [ValidateSet('Debug','Release')][string]$Configuration = 'Debug'
)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$buildPath = if($Configuration -eq 'Release') {
  'build/windows/shared_release_ninja/runner/Release/alera-dev.exe'
} else { 'build/windows/shared_gui_ninja/runner/Debug/alera-dev.exe' }
$expectedExe = Join-Path $root $buildPath
$target = @(Get-Process alera-dev -ErrorAction SilentlyContinue |
  Where-Object { $_.Path -eq $expectedExe -and ($TargetPid -eq 0 -or $_.Id -eq $TargetPid) })
if ($target.Count -ne 1) {
  throw "Exactly one isolated test Alera process required. Found $($target.Count)"
}
$app = $target[0]
if ($app.MainWindowHandle -eq 0) { throw 'Isolated Alera has no HWND' }
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class AleraE2ECaptureNative {
 public delegate bool EnumCallback(IntPtr hwnd,IntPtr value);
 [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left,Top,Right,Bottom; }
 [DllImport("user32.dll")] public static extern bool EnumWindows(EnumCallback callback,IntPtr data);
 [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd,out uint pid);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hwnd,StringBuilder text,int max);
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd, out RECT rect);
 [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hwnd, IntPtr hdc, uint flags);
 public static IntPtr Find(int pid,string title) {
  IntPtr found=IntPtr.Zero;
  EnumWindows((hwnd,_)=>{
   uint owner;GetWindowThreadProcessId(hwnd,out owner);
   if(owner != (uint)pid)return true;
   var sb=new StringBuilder(256);
   GetWindowText(hwnd,sb,sb.Capacity);
   if(sb.ToString()==title){found=hwnd;return false;}
   return true;
  },IntPtr.Zero);
  return found;
 }
}
'@
$hwnd = $app.MainWindowHandle
if($WindowTitle -ne '') {
  $hwnd = [AleraE2ECaptureNative]::Find($app.Id, $WindowTitle)
  if($hwnd -eq [IntPtr]::Zero){throw "Isolated Alera does not own window title $WindowTitle"}
}
$rect = New-Object AleraE2ECaptureNative+RECT
if (-not [AleraE2ECaptureNative]::GetWindowRect($hwnd, [ref]$rect)) {
  throw 'GetWindowRect failed'
}
$width=$rect.Right-$rect.Left; $height=$rect.Bottom-$rect.Top
if ($width -lt 100 -or $height -lt 100 -or $width -gt 5500 -or $height -gt 4000) {
  throw "Invalid HWND bounds: $width x $height"
}
$bitmap = New-Object System.Drawing.Bitmap($width, $height)
try {
  $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
  try {
    $dc = $graphics.GetHdc()
    try { $rendered = [AleraE2ECaptureNative]::PrintWindow($hwnd,$dc,2) }
    finally { $graphics.ReleaseHdc($dc) }
  } finally { $graphics.Dispose() }
  $dir = if($Configuration -eq 'Release') {
    Join-Path $root 'build/windows/shared_release_ninja/diagnostics'
  } else { Join-Path $root 'build/windows/shared_gui_ninja/diagnostics' }
  New-Item -ItemType Directory -Force -Path $dir | Out-Null
  if ($OutputName -notmatch '^[A-Za-z0-9_-]+\.png$') {
    throw 'OutputName must be a bare .png filename'
  }
  $out = Join-Path $dir $OutputName
  $bitmap.Save($out, [System.Drawing.Imaging.ImageFormat]::Png)
  Write-Output "PRINTWINDOW_OK=$rendered PID=$($app.Id) HWND=$hwnd SIZE=${width}x$height FILE=$out"
} finally { $bitmap.Dispose() }
