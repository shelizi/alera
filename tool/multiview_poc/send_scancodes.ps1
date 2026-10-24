param(
  [Parameter(Mandatory=$true)][ValidateRange(1,2147483647)][int]$TargetPid,
  [Parameter(Mandatory=$true)][long]$Hwnd,
  [Parameter(Mandatory=$true)][string]$Scans,
  [int]$GapMs = 120
)
$ErrorActionPreference='Stop'
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class AleraScanSend {
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern uint SendInput(uint n, INPUT[] p, int cb);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint id);
  [StructLayout(LayoutKind.Sequential)] public struct KEY { public ushort vk, scan; public uint flags, time; public UIntPtr extra; }
  [StructLayout(LayoutKind.Explicit, Size=40)] public struct INPUT { [FieldOffset(0)] public uint type; [FieldOffset(8)] public KEY key; }
}
'@
[uint32]$owner=0
[void][AleraScanSend]::GetWindowThreadProcessId([IntPtr]$Hwnd,[ref]$owner)
if($owner -ne [uint32]$TargetPid){ throw "HWND $Hwnd belongs to PID $owner, not $TargetPid" }
[void][AleraScanSend]::SetForegroundWindow([IntPtr]$Hwnd)
Start-Sleep -Milliseconds 250
foreach($scan in ($Scans -split ',' | ForEach-Object { [int]$_.Trim() })){
  $items=[AleraScanSend+INPUT[]]::CreateInstance([AleraScanSend+INPUT],2)
  $items[0].type=1; $items[0].key.scan=[ushort]$scan; $items[0].key.flags=8 # SCANCODE
  $items[1].type=1; $items[1].key.scan=[ushort]$scan; $items[1].key.flags=10 # SCANCODE|KEYUP
  $sent=[AleraScanSend]::SendInput(2,$items,[Runtime.InteropServices.Marshal]::SizeOf([type][AleraScanSend+INPUT]))
  Write-Output "scan=0x$($scan.ToString('X2')) injected=$sent"
  Start-Sleep -Milliseconds $GapMs
}
