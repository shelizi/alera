Add-Type @'
using System;using System.Text;using System.Runtime.InteropServices;
public class FGQ { [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h,StringBuilder s,int n);
 [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h,out uint o); }
'@
$fg=[FGQ]::GetForegroundWindow(); $o=0;[void][FGQ]::GetWindowThreadProcessId($fg,[ref]$o)
$b=New-Object Text.StringBuilder 256;[void][FGQ]::GetWindowText($fg,$b,256)
Write-Output ("FOREGROUND HWND=" + $fg + " PID=" + $o + " TITLE=" + $b.ToString())
