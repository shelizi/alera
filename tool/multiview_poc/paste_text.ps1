param(
  [Parameter(Mandatory=$true)][long]$Hwnd,
  [Parameter(Mandatory=$true)][string]$Text,
  [switch]$NoEnter
)
$ErrorActionPreference='Stop'
Set-Clipboard -Value $Text
Add-Type @'
using System;using System.Runtime.InteropServices;
public static class PasteK {
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
}
'@
[void][PasteK]::SetForegroundWindow([IntPtr]$Hwnd)
Start-Sleep -Milliseconds 400
[PasteK]::keybd_event(0x11,0,0,[UIntPtr]::Zero)          # Ctrl down
[PasteK]::keybd_event(0x56,0,0,[UIntPtr]::Zero)          # V down
[PasteK]::keybd_event(0x56,0,2,[UIntPtr]::Zero)          # V up
[PasteK]::keybd_event(0x11,0,2,[UIntPtr]::Zero)          # Ctrl up
Start-Sleep -Milliseconds 500
if(-not $NoEnter){
  [PasteK]::keybd_event(0x0D,0,0,[UIntPtr]::Zero)
  [PasteK]::keybd_event(0x0D,0,2,[UIntPtr]::Zero)
}
Write-Output "PASTED len=$($Text.Length)"
