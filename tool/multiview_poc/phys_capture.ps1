param(
  [Parameter(Mandatory=$true)][long]$Hwnd,
  [Parameter(Mandatory=$true)][string]$OutFile
)
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;using System.Runtime.InteropServices;
public class PhysCap {
  [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h,out RECT r);
  public struct RECT { public int left,top,right,bottom; }
}
'@
[void][PhysCap]::SetThreadDpiAwarenessContext([IntPtr]::New(-4))
$r = New-Object PhysCap+RECT
[void][PhysCap]::GetWindowRect([IntPtr]$Hwnd,[ref]$r)
$w = $r.right-$r.left; $h=$r.bottom-$r.top
$bmp = New-Object Drawing.Bitmap $w,$h
$g = [Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($r.left,$r.top,0,0,$bmp.Size)
$bmp.Save($OutFile,[Drawing.Imaging.ImageFormat]::Png)
"PHYS_CAPTURE ${w}x${h} at $($r.left),$($r.top) -> $OutFile"
