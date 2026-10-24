param(
  [Parameter(Mandatory=$true)][int]$X,
  [Parameter(Mandatory=$true)][int]$Y,
  [Parameter(Mandatory=$true)][string]$OutFile
)
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;using System.Runtime.InteropServices;
public static class CursorProbe {
  [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern bool GetCursorPos(out P p);
  public struct P { public int X, Y; }
}
'@
[void][CursorProbe]::SetThreadDpiAwarenessContext([IntPtr]::New(-4))
[void][CursorProbe]::SetCursorPos($X, $Y)
Start-Sleep -Milliseconds 300
$p = New-Object CursorProbe+P
[void][CursorProbe]::GetCursorPos([ref]$p)
"CURSOR_AT phys=$($p.X),$($p.Y)"
# Capture a physical region around the point so the arrow is visible.
$size = 700
$left = $p.X - 200
$top = $p.Y - 200
$bmp = New-Object Drawing.Bitmap $size, $size
$g = [Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($left, $top, 0, 0, $bmp.Size)
$bmp.Save($OutFile, [Drawing.Imaging.ImageFormat]::Png)
"REGION phys=$left,$top ${size}x${size} -> $OutFile"
