param([long]$Hwnd)
Add-Type @'
using System;using System.Runtime.InteropServices;
public class WRQ { [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h,out RECT r); public struct RECT{public int l,t,r,b;} }
'@
$r=New-Object WRQ+RECT
[void][WRQ]::GetWindowRect([IntPtr]$Hwnd,[ref]$r)
Write-Output ("HWND=" + $Hwnd + " rect=" + $r.l + "," + $r.t + "-" + $r.r + "," + $r.b + " size=" + ($r.r-$r.l) + "x" + ($r.b-$r.t))
