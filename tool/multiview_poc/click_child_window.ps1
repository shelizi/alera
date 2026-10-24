param(
  [Parameter(Mandatory=$true)][ValidateRange(1,2147483647)][int]$TargetPid,
  [Parameter(Mandatory=$true)][ValidateRange(1,4294967295)][long]$Hwnd,
  [Parameter(Mandatory=$true)][ValidateRange(0,1)][double]$RelX,
  [Parameter(Mandatory=$true)][ValidateRange(0,1)][double]$RelY
)
$ErrorActionPreference='Stop'
Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class AleraChildClick {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr l);
  [DllImport("user32.dll")] public static extern int GetWindowTextW(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint id);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint dx, uint dy, uint d, UIntPtr e);
  [DllImport("user32.dll")] public static extern IntPtr SendMessageW(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
  [DllImport("user32.dll")] public static extern bool ScreenToClient(IntPtr h, ref POINT p);
  [DllImport("user32.dll")] public static extern IntPtr RealChildWindowFromPoint(IntPtr h, POINT p);
  [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern int GetClassNameW(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern int GetSystemMetrics(int i);
  [DllImport("user32.dll")] public static extern uint SendInput(uint n, INPUT[] p, int cb);
  [StructLayout(LayoutKind.Sequential)] public struct MOUSEINPUT { public int dx, dy; public uint mouseData, dwFlags, time; public UIntPtr dwExtraInfo; }
  [StructLayout(LayoutKind.Sequential)] public struct INPUT { public uint type; public MOUSEINPUT mi; }
  public struct POINT { public int X, Y; }
  public struct RECT { public int Left, Top, Right, Bottom; }
  public static IntPtr Found = IntPtr.Zero;
}
'@
[void][AleraChildClick]::SetThreadDpiAwarenessContext([IntPtr]::New(-4))
[AleraChildClick]::Found=[IntPtr]$Hwnd
if(-not [AleraChildClick]::IsWindowVisible([AleraChildClick]::Found)){ throw "HWND $Hwnd is not a visible window" }
[uint32]$owner=0
[void][AleraChildClick]::GetWindowThreadProcessId([AleraChildClick]::Found,[ref]$owner)
if($owner -ne [uint32]$TargetPid){ throw "HWND $Hwnd belongs to PID $owner, not $TargetPid" }
$r = New-Object AleraChildClick+RECT
[void][AleraChildClick]::GetWindowRect([AleraChildClick]::Found,[ref]$r)
$w=$r.Right-$r.Left; $h=$r.Bottom-$r.Top
$pt=New-Object AleraChildClick+POINT
$pt.X=[int]($r.Left + $w*$RelX); $pt.Y=[int]($r.Top + $h*$RelY)
Write-Output "child=$([AleraChildClick]::Found) rect=${w}x${h} screen=$($pt.X),$($pt.Y)"
# Real injection: Flutter consumes WM_POINTER* which the OS synthesizes only for
# genuine input. SetCursorPos + SendInput absolute move+click covers that.
[void][AleraChildClick]::SetForegroundWindow([AleraChildClick]::Found)
Start-Sleep -Milliseconds 250
$input1=New-Object AleraChildClick+INPUT
$input1.type=1
$input1.mi=New-Object AleraChildClick+MOUSEINPUT
$vLeft=[AleraChildClick]::GetSystemMetrics(76); $vTop=[AleraChildClick]::GetSystemMetrics(77)
$vW=[AleraChildClick]::GetSystemMetrics(78); $vH=[AleraChildClick]::GetSystemMetrics(79)
$input1.mi.dx=[int](($pt.X - $vLeft)*65535/$vW)
$input1.mi.dy=[int](($pt.Y - $vTop)*65535/$vH)
$input1.mi.dwFlags=0x8000 -bor 0x4000 -bor 0x0001 # ABSOLUTE|VIRTUALDESK|MOVE
$input2=New-Object AleraChildClick+INPUT
$input2.type=1; $input2.mi=New-Object AleraChildClick+MOUSEINPUT
$input2.mi.dwFlags=0x0002 # LEFTDOWN
$input3=New-Object AleraChildClick+INPUT
$input3.type=1; $input3.mi=New-Object AleraChildClick+MOUSEINPUT
$input3.mi.dwFlags=0x0004 # LEFTUP
$batch=[AleraChildClick+INPUT[]]@($input1,$input2,$input3)
$sent=[AleraChildClick]::SendInput(3,$batch,[Runtime.InteropServices.Marshal]::SizeOf([type][AleraChildClick+INPUT]))
Write-Output "CHILD_CLICK_SENT injected=$sent"
