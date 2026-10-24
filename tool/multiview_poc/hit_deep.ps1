param(
  [Parameter(Mandatory=$true)][int]$X,
  [Parameter(Mandatory=$true)][int]$Y
)
Add-Type @'
using System;using System.Runtime.InteropServices;using System.Text;
public static class HitDeep {
  [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
  [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(P p);
  [DllImport("user32.dll")] public static extern IntPtr GetAncestor(IntPtr h, uint f);
  [DllImport("user32.dll")] public static extern IntPtr GetParent(IntPtr h);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out R r);
  [DllImport("user32.dll")] public static extern int GetClassNameW(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern int GetWindowTextW(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  public struct P { public int X, Y; }
  public struct R { public int L,T,Rt,B; }
}
'@
[void][HitDeep]::SetThreadDpiAwarenessContext([IntPtr]::New(-4))
$p = New-Object HitDeep+P
$p.X = $X; $p.Y = $Y
$chain = @()
$h = [HitDeep]::WindowFromPoint($p)
while ($h -ne [IntPtr]::Zero -and $chain.Count -lt 8) {
  $cn = New-Object Text.StringBuilder 128
  [void][HitDeep]::GetClassNameW($h, $cn, 128)
  $tt = New-Object Text.StringBuilder 128
  [void][HitDeep]::GetWindowTextW($h, $tt, 128)
  [uint32]$pid_ = 0
  [void][HitDeep]::GetWindowThreadProcessId($h, [ref]$pid_)
  $r = New-Object HitDeep+R
  [void][HitDeep]::GetWindowRect($h, [ref]$r)
  $chain += "hwnd=$h pid=$pid_ cls=$($cn.ToString()) title='$($tt.ToString())' vis=$([HitDeep]::IsWindowVisible($h)) rect=$($r.L),$($r.T)-$($r.Rt),$($r.B)"
  $h = [HitDeep]::GetParent($h)
}
$chain
