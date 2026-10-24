param(
  [Parameter(Mandatory=$true)][long]$Hwnd,
  [Parameter(Mandatory=$true)][string]$Text,
  [switch]$NoEnter
)
$ErrorActionPreference='Stop'
Add-Type @'
using System;using System.Runtime.InteropServices;
public static class TyEn {
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
  [DllImport("user32.dll")] public static extern short VkKeyScan(char ch);
}
'@
[void][TyEn]::SetForegroundWindow([IntPtr]$Hwnd)
Start-Sleep -Milliseconds 350
# Toggle CJK IME to alphanumeric mode: press Shift once (MS IME Shift = EN/CJK toggle)
[TyEn]::keybd_event(0x10,0,0,[UIntPtr]::Zero)
[TyEn]::keybd_event(0x10,0,2,[UIntPtr]::Zero)
Start-Sleep -Milliseconds 300
foreach($ch in $Text.ToCharArray()){
  $vk = [TyEn]::VkKeyScan($ch)
  if($vk -lt 0){ continue }
  $code = $vk -band 0xFF
  $shift = ($vk -shr 8) -band 1
  if($shift){ [TyEn]::keybd_event(0x10,0,0,[UIntPtr]::Zero) }
  [TyEn]::keybd_event([byte]$code,0,0,[UIntPtr]::Zero)
  [TyEn]::keybd_event([byte]$code,0,2,[UIntPtr]::Zero)
  if($shift){ [TyEn]::keybd_event(0x10,0,2,[UIntPtr]::Zero) }
  Start-Sleep -Milliseconds 25
}
if(-not $NoEnter){
  Start-Sleep -Milliseconds 120
  [TyEn]::keybd_event(0x0D,0,0,[UIntPtr]::Zero)
  [TyEn]::keybd_event(0x0D,0,2,[UIntPtr]::Zero)
}
Write-Output "TYPED_EN len=$($Text.Length)"
