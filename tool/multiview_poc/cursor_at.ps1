param([int]$X,[int]$Y)
Add-Type @'
using System;using System.Runtime.InteropServices;
public class CA {
  [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x,int y);
  [DllImport("user32.dll")] public static extern bool GetCursorInfo(ref CURSORINFO i);
  [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(POINT p);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  public struct POINT { public int X, Y; }
  public struct CURSORINFO { public int cbSize; public int flags; public IntPtr hCursor; public POINT pt; }
}
'@
[void][CA]::SetThreadDpiAwarenessContext([IntPtr]::New(-4))
[void][CA]::SetCursorPos($X,$Y);Start-Sleep -Milliseconds 300
$ci=New-Object CA+CURSORINFO;$ci.cbSize=[Runtime.InteropServices.Marshal]::SizeOf($ci)
[void][CA]::GetCursorInfo([ref]$ci)
$pt=New-Object CA+POINT;$pt.X=$X;$pt.Y=$Y
$hw=[CA]::WindowFromPoint($pt)
$pid=0;[void][CA]::GetWindowThreadProcessId($hw,[ref]$pid)
"cursor=$($ci.hCursor) hwnd=$hw pid=$pid"
