param(
  [Parameter(Mandatory=$true)][long]$Hwnd,
  [Parameter(Mandatory=$true)][string]$Text
)
Add-Type @'
using System;using System.Runtime.InteropServices;
public class KeySender {
  [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr h,uint m,IntPtr w,IntPtr l);
  [DllImport("user32.dll")] public static extern uint MapVirtualKeyW(uint code,uint map);
}
'@
foreach($ch in $Text.ToCharArray()){
  $vk = [byte]$ch
  if($ch -ge 'a' -and $ch -le 'z'){ $vk = [byte]([char]::ToUpper($ch)) }
  $scan = [KeySender]::MapVirtualKeyW([uint32]$vk,0)
  $lp = 1 -bor ($scan -shl 16)
  [void][KeySender]::PostMessageW([IntPtr]$Hwnd,0x0100,[IntPtr]$vk,[IntPtr]$lp)
  [void][KeySender]::PostMessageW([IntPtr]$Hwnd,0x0101,[IntPtr]$vk,[IntPtr]($lp -bor 0xC0000000))
}
"SENT_WMKEY text=$Text"
