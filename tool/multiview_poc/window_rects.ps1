param(
  [Parameter(Mandatory=$true)][string]$Hwnds
)
$ErrorActionPreference='Stop'
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class AleraRects {
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
  public struct RECT { public int Left, Top, Right, Bottom; }
}
'@
foreach($h in ($Hwnds -split ',' | ForEach-Object { [long]$_.Trim() })){
  $r=New-Object AleraRects+RECT
  [void][AleraRects]::GetWindowRect([IntPtr]$h,[ref]$r)
  $w=$r.Right-$r.Left; $hh=$r.Bottom-$r.Top
  Write-Output "hwnd=$h iconic=$([AleraRects]::IsIconic([IntPtr]$h)) rect=${w}x${hh} at $($r.Left),$($r.Top)"
}
