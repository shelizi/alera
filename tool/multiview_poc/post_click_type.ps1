param(
  [Parameter(Mandatory=$true)][long]$View,
  [int]$X=0,
  [int]$Y=0,
  [string]$Text=''
)
Add-Type @'
using System;using System.Runtime.InteropServices;
public class PCT {
  [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr h,uint m,UIntPtr w,IntPtr l);
}
'@
function PackXY($x,$y){ return [IntPtr](($y -band 0xffff) -shl 16 -bor ($x -band 0xffff)) }
$v = [IntPtr]$View
if($X -gt 0 -or $Y -gt 0){
  $lp = PackXY $X $Y
  [void][PCT]::PostMessageW($v,0x0200,[UIntPtr]::Zero,$lp)       # WM_MOUSEMOVE
  [void][PCT]::PostMessageW($v,0x0201,[UIntPtr]1,$lp)            # WM_LBUTTONDOWN
  [void][PCT]::PostMessageW($v,0x0202,[UIntPtr]::Zero,$lp)       # WM_LBUTTONUP
  "CLICK_POSTED view=$v client=$X,$Y"
}
if($Text.Length -gt 0){
  $n=0
  foreach($c in $Text.ToCharArray()){
    if([PCT]::PostMessageW($v,0x0102,[UIntPtr][int][char]$c,[IntPtr]::Zero)){ $n+=1 }
  }
  "CHAR_POSTED=$n view=$v text=$Text"
}
