param(
  [Parameter(Mandatory=$true)][long]$Hwnd,
  [Parameter(Mandatory=$true)][string]$Text,
  [switch]$NoEnter
)
$ErrorActionPreference='Stop'
Add-Type @'
using System;using System.Runtime.InteropServices;
public static class TyUS {
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr LoadKeyboardLayoutW(string pwszKLID, uint Flags);
  [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr h,uint m,IntPtr w,IntPtr l);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
  [DllImport("user32.dll")] public static extern short VkKeyScanW(char ch);
}
'@
[void][TyUS]::SetForegroundWindow([IntPtr]$Hwnd)
Start-Sleep -Milliseconds 300
# Request the US English layout for this window's input locale
$hkl = [TyUS]::LoadKeyboardLayoutW('00000409', 1)  # KLF_ACTIVATE
[void][TyUS]::PostMessageW([IntPtr]$Hwnd, 0x0050, [IntPtr]0, $hkl)  # WM_INPUTLANGCHANGEREQUEST
Start-Sleep -Milliseconds 400
foreach($ch in $Text.ToCharArray()){
  $r = [TyUS]::VkKeyScanW($ch)
  if($r -lt 0){ Write-Output "skip $ch"; continue }
  $vk=[byte]($r -band 0xFF); $shift=($r -shr 8) -band 1
  if($shift){ [TyUS]::keybd_event(0x10,0,0,[UIntPtr]::Zero) }
  [TyUS]::keybd_event($vk,0,0,[UIntPtr]::Zero)
  Start-Sleep -Milliseconds 20
  [TyUS]::keybd_event($vk,0,2,[UIntPtr]::Zero)
  if($shift){ [TyUS]::keybd_event(0x10,0,2,[UIntPtr]::Zero) }
  Start-Sleep -Milliseconds 20
}
if(-not $NoEnter){
  [TyUS]::keybd_event(0x0D,0,0,[UIntPtr]::Zero)
  Start-Sleep -Milliseconds 40
  [TyUS]::keybd_event(0x0D,0,2,[UIntPtr]::Zero)
}
Write-Output "TYPED_US len=$($Text.Length)+enter"
