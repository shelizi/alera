param(
  [Parameter(Mandatory=$true)][int]$X,
  [Parameter(Mandatory=$true)][int]$Y,
  [ValidateSet('left','right')][string]$Button = 'left',
  [long]$Foreground = 0
)
Add-Type @'
using System;using System.Runtime.InteropServices;
public class RealClick {
  [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern uint SendInput(uint n, INPUT[] p, int cb);
  [StructLayout(LayoutKind.Sequential)] public struct MOUSE { public int dx,dy; public uint data,flags,time; public UIntPtr extra; }
  [StructLayout(LayoutKind.Explicit, Size=40)] public struct INPUT { [FieldOffset(0)] public uint type; [FieldOffset(4)] public int pad; [FieldOffset(8)] public MOUSE m; }
}
'@
[void][RealClick]::SetThreadDpiAwarenessContext([IntPtr]::New(-4))
if ($Foreground -ne 0) {
  [void][RealClick]::SetForegroundWindow([IntPtr]$Foreground)
  Start-Sleep -Milliseconds 400
}
[void][RealClick]::SetCursorPos($X, $Y)
Start-Sleep -Milliseconds 200
$down = 0x0002; $up = 0x0004
if ($Button -eq 'right') { $down = 0x0008; $up = 0x0010 }
$items = New-Object 'RealClick+INPUT[]' 2
$items[0].type=0; $items[0].m.flags=$down
$items[1].type=0; $items[1].m.flags=$up
$sent=[RealClick]::SendInput(2,$items,[Runtime.InteropServices.Marshal]::SizeOf([type][RealClick+INPUT]))
"RCLICK_SENT button=$Button phys=$X,$Y injected=$sent"
