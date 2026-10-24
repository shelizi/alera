param(
  [Parameter(Mandatory=$true)][long]$View,
  [Parameter(Mandatory=$true)][int]$X,
  [Parameter(Mandatory=$true)][int]$Y
)
Add-Type @'
using System;using System.Runtime.InteropServices;
public class RCT {
  [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr h,uint m,UIntPtr w,IntPtr l);
}
'@
function PackXY($x,$y){ return [IntPtr](($y -band 0xffff) -shl 16 -bor ($x -band 0xffff)) }
$v=[IntPtr]$View
$lp=PackXY $X $Y
[void][RCT]::PostMessageW($v,0x0200,[UIntPtr]::Zero,$lp)          # WM_MOUSEMOVE
[void][RCT]::PostMessageW($v,0x0204,[UIntPtr]2,$lp)               # WM_RBUTTONDOWN
[void][RCT]::PostMessageW($v,0x0205,[UIntPtr]::Zero,$lp)          # WM_RBUTTONUP
"RCLICK_POSTED view=$v client=$X,$Y"
