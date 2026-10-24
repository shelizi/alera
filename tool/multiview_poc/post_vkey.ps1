param(
  [Parameter(Mandatory=$true)][long]$View,
  [Parameter(Mandatory=$true)][int]$Vk,
  [int]$Scan = 0
)
Add-Type @'
using System;using System.Runtime.InteropServices;
public class PVK {
  [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr h,uint m,UIntPtr w,IntPtr l);
  [DllImport("user32.dll")] public static extern uint MapVirtualKeyW(uint u,uint m);
}
'@
if($Scan -eq 0){ $Scan = [int][PVK]::MapVirtualKeyW([uint32]$Vk,0) }
$down = [IntPtr]((1) -bor ($Scan -shl 16) -bor (1 -shl 24))
$up   = [IntPtr]((1) -bor ($Scan -shl 16) -bor (1 -shl 24) -bor (3 -shl 30))
$v=[IntPtr]$View
$a=[PVK]::PostMessageW($v,0x0100,[UIntPtr]$Vk,$down)
$b=[PVK]::PostMessageW($v,0x0101,[UIntPtr]$Vk,$up)
"VKEY vk=0x{0:X2} scan=0x{1:X2} down={2} up={3}" -f $Vk,$Scan,$a,$b
