param(
  [Parameter(Mandatory=$true)][int]$X,
  [Parameter(Mandatory=$true)][int]$Y
)
Add-Type @'
using System;using System.Runtime.InteropServices;using System.Text;
public static class HitPoint {
  [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
  [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(P p);
  [DllImport("user32.dll")] public static extern IntPtr GetAncestor(IntPtr h, uint f);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out R r);
  [DllImport("user32.dll")] public static extern int GetClassNameW(IntPtr h, StringBuilder s, int n);
  public struct P { public int X, Y; }
  public struct R { public int L,T,Rt,B; }
}
'@
# Physical-pixel coordinates: -4 = PER_MONITOR_AWARE_V2.
[void][HitPoint]::SetThreadDpiAwarenessContext([IntPtr]::New(-4))
$p = New-Object HitPoint+P
$p.X = $X; $p.Y = $Y
$h = [HitPoint]::WindowFromPoint($p)
$root = [HitPoint]::GetAncestor($h, 2)
if ($root -ne [IntPtr]::Zero) { $h = $root }
$cn = New-Object Text.StringBuilder 128
[void][HitPoint]::GetClassNameW($h, $cn, 128)
$r = New-Object HitPoint+R
[void][HitPoint]::GetWindowRect($h, [ref]$r)
"phys=$X,$Y root=$h class=$($cn.ToString()) rect=$($r.L),$($r.T)-$($r.Rt),$($r.B)"
