param(
  [Parameter(Mandatory=$true)][long]$Hwnd,
  [Parameter(Mandatory=$true)][string]$Text,
  [switch]$NoEnter,
  [switch]$RestoreIme
)
$ErrorActionPreference='Stop'
Add-Type @'
using System;using System.Runtime.InteropServices;
public static class TyRaw {
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetFocus();
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
  [DllImport("user32.dll")] public static extern short VkKeyScanW(char ch);
  [DllImport("imm32.dll")] public static extern IntPtr ImmGetContext(IntPtr h);
  [DllImport("imm32.dll")] public static extern IntPtr ImmAssociateContext(IntPtr h, IntPtr c);
  [DllImport("imm32.dll")] public static extern bool ImmReleaseContext(IntPtr h, IntPtr c);
  [DllImport("imm32.dll")] public static extern bool ImmSetOpenStatus(IntPtr c, bool open);
}
'@
[void][TyRaw]::SetForegroundWindow([IntPtr]$Hwnd)
Start-Sleep -Milliseconds 350
# Detach IME from the target window so VkKeyScan chars reach the terminal raw.
$saved = [TyRaw]::ImmAssociateContext([IntPtr]$Hwnd, [IntPtr]::Zero)
Write-Output "IME detached prev=$saved"
foreach($ch in $Text.ToCharArray()){
  $r = [TyRaw]::VkKeyScanW($ch)
  if($r -lt 0){ Write-Output "skip $ch"; continue }
  $vk=[byte]($r -band 0xFF); $shift=($r -shr 8) -band 1
  if($shift){ [TyRaw]::keybd_event(0x10,0,0,[UIntPtr]::Zero) }
  [TyRaw]::keybd_event($vk,0,0,[UIntPtr]::Zero)
  Start-Sleep -Milliseconds 18
  [TyRaw]::keybd_event($vk,0,2,[UIntPtr]::Zero)
  if($shift){ [TyRaw]::keybd_event(0x10,0,2,[UIntPtr]::Zero) }
  Start-Sleep -Milliseconds 18
}
if(-not $NoEnter){
  [TyRaw]::keybd_event(0x0D,0,0,[UIntPtr]::Zero)
  Start-Sleep -Milliseconds 40
  [TyRaw]::keybd_event(0x0D,0,2,[UIntPtr]::Zero)
}
if($RestoreIme -and $saved -ne [IntPtr]::Zero){
  [void][TyRaw]::ImmAssociateContext([IntPtr]$Hwnd, $saved)
}
Write-Output "TYPED_RAW len=$($Text.Length)+enter"
