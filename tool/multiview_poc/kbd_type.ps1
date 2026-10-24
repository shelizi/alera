param(
  [Parameter(Mandatory=$true)][long]$Hwnd,
  [Parameter(Mandatory=$true)][string]$Text
)
$ErrorActionPreference='Stop'
Add-Type @'
using System;using System.Runtime.InteropServices;
public class KbdT {
  [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
  [DllImport("user32.dll")] public static extern short VkKeyScan(char ch);
}
'@
[void][KbdT]::SetThreadDpiAwarenessContext([IntPtr]::New(-4))
[void][KbdT]::SetForegroundWindow([IntPtr]$Hwnd)
Start-Sleep -Milliseconds 300
foreach($ch in $Text.ToCharArray()){
  $r=[KbdT]::VkKeyScan($ch)
  if($r -lt 0){ Write-Output "skip $ch"; continue }
  $vk=[byte]($r -band 0xFF); $shift=($r -shr 8) -band 1
  if($shift){ [KbdT]::keybd_event(0x10,0,0,[UIntPtr]::Zero) }
  [KbdT]::keybd_event($vk,0,0,[UIntPtr]::Zero)
  Start-Sleep -Milliseconds 30
  [KbdT]::keybd_event($vk,0,2,[UIntPtr]::Zero)
  if($shift){ [KbdT]::keybd_event(0x10,0,2,[UIntPtr]::Zero) }
  Start-Sleep -Milliseconds 30
}
if($Text.EndsWith("`r")){} else { [KbdT]::keybd_event(0x0D,0,0,[UIntPtr]::Zero); sleep -m 40; [KbdT]::keybd_event(0x0D,0,2,[UIntPtr]::Zero) }
Write-Output "TYPED $($Text.Length)+enter"
