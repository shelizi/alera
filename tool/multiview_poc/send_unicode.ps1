param(
  [Parameter(Mandatory=$true)][int]$TargetPid,
  [Parameter(Mandatory=$true)][long]$Hwnd,
  [Parameter(Mandatory=$true)][string]$Text,
  [int]$GapMs = 60
)
$ErrorActionPreference='Stop'
Add-Type @'
using System;using System.Runtime.InteropServices;
public static class UniSend {
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern uint SendInput(uint n, INPUT[] p, int cb);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint id);
  [StructLayout(LayoutKind.Sequential)] public struct KEY { public ushort vk, scan; public uint flags, time; public IntPtr extra; }
  [StructLayout(LayoutKind.Explicit, Size=40)] public struct INPUT { [FieldOffset(0)] public uint type; [FieldOffset(8)] public KEY key; }
}
'@
[uint32]$owner=0
[void][UniSend]::GetWindowThreadProcessId([IntPtr]$Hwnd,[ref]$owner)
if($owner -ne [uint32]$TargetPid){ throw "HWND $Hwnd belongs to PID $owner, not $TargetPid" }
[void][UniSend]::SetForegroundWindow([IntPtr]$Hwnd)
Start-Sleep -Milliseconds 250
foreach($ch in $Text.ToCharArray()){
  $items=[UniSend+INPUT[]]::CreateInstance([UniSend+INPUT],2)
  $items[0].type=1; $items[0].key.scan=[int][char]$ch; $items[0].key.flags=4 # KEYEVENTF_UNICODE
  $items[1].type=1; $items[1].key.scan=[int][char]$ch; $items[1].key.flags=6 # UNICODE|KEYUP
  $sent=[UniSend]::SendInput(2,$items,[Runtime.InteropServices.Marshal]::SizeOf([type][UniSend+INPUT]))
  if($sent -ne 2){ Write-Output "fail ch=$ch sent=$sent" }
  Start-Sleep -Milliseconds $GapMs
}
# Enter key via VK
$items=[UniSend+INPUT[]]::CreateInstance([UniSend+INPUT],2)
$items[0].type=1; $items[0].key.vk=0x0D
$items[1].type=1; $items[1].key.vk=0x0D; $items[1].key.flags=2
[void][UniSend]::SendInput(2,$items,40)
Write-Output "UNICODE_TYPED len=$($Text.Length)+enter"
