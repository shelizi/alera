param(
  [Parameter(Mandatory=$true)][long]$Hwnd
)
$ErrorActionPreference='Stop'
Add-Type @'
using System;using System.Runtime.InteropServices;
public static class PQ {
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h,uint m,UIntPtr w,IntPtr l);
}
'@
$ok=[PQ]::PostMessage([IntPtr]$Hwnd,0x111,[UIntPtr]::op_Explicit(1004),[IntPtr]::Zero)
Write-Output "POSTED ok=$ok"
