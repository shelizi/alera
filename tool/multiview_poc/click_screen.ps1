param(
  [Parameter(Mandatory=$true)][long]$Hwnd,
  [Parameter(Mandatory=$true)][int]$ScreenX,
  [Parameter(Mandatory=$true)][int]$ScreenY
)
Add-Type @'
using System;using System.Runtime.InteropServices;
public class ScreenClick {
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint dx, uint dy, uint d, UIntPtr e);
  [DllImport("user32.dll")] public static extern uint SendInput(uint n, INPUT[] p, int cb);
  [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
  [StructLayout(LayoutKind.Sequential)] public struct MOUSE { public int dx,dy; public uint data,flags,time; public UIntPtr extra; }
  [StructLayout(LayoutKind.Explicit, Size=40)] public struct INPUT { [FieldOffset(0)] public uint type; [FieldOffset(4)] public int pad; [FieldOffset(8)] public MOUSE m; }
}
'@
[void][ScreenClick]::SetThreadDpiAwarenessContext([IntPtr]::New(-4))
[void][ScreenClick]::SetForegroundWindow([IntPtr]$Hwnd)
Start-Sleep -Milliseconds 400
[void][ScreenClick]::SetCursorPos($ScreenX,$ScreenY)
Start-Sleep -Milliseconds 150
$items = New-Object 'ScreenClick+INPUT[]' 2
$items[0].type=0; $items[0].m.flags=0x0002 # LEFTDOWN
$items[1].type=0; $items[1].m.flags=0x0004 # LEFTUP
$sent=[ScreenClick]::SendInput(2,$items,[Runtime.InteropServices.Marshal]::SizeOf([type][ScreenClick+INPUT]))
"CLICK_SENT screen=$ScreenX,$ScreenY injected=$sent"
