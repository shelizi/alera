param(
  [Parameter(Mandatory=$true)][long]$Hwnd,
  [Parameter(Mandatory=$true)][string]$OutFile
)
$ErrorActionPreference='Stop'
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class AleraAnyCapture {
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint f);
  [DllImport("user32.dll")] public static extern IntPtr GetWindowDC(IntPtr h);
  [DllImport("user32.dll")] public static extern int ReleaseDC(IntPtr h, IntPtr dc);
  public struct RECT { public int Left, Top, Right, Bottom; }
}
'@
Add-Type -AssemblyName System.Drawing
$r=New-Object AleraAnyCapture+RECT
[void][AleraAnyCapture]::GetWindowRect([IntPtr]$Hwnd,[ref]$r)
$w=$r.Right-$r.Left; $h=$r.Bottom-$r.Top
$bmp=New-Object System.Drawing.Bitmap $w,$h
$g=[System.Drawing.Graphics]::FromImage($bmp)
$dc=$g.GetHdc()
$ok=[AleraAnyCapture]::PrintWindow([IntPtr]$Hwnd,$dc,2)
$g.ReleaseHdc($dc); $g.Dispose()
$bmp.Save($OutFile,[System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
Write-Output "CAPTURE ok=$ok size=${w}x${h} file=$OutFile"
