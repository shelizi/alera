param([Parameter(Mandatory=$true)][long]$Hwnd)
Add-Type @'
using System;using System.Runtime.InteropServices;using System.Text;
public static class WI {
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
  [DllImport("user32.dll")] public static extern int GetClassNameW(IntPtr h,StringBuilder s,int n);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h,out uint p);
  [DllImport("user32.dll")] public static extern int GetWindowTextW(IntPtr h,StringBuilder s,int n);
}
'@
$h=[IntPtr]$Hwnd
$c=New-Object Text.StringBuilder 256
$t=New-Object Text.StringBuilder 256
[void][WI]::GetClassNameW($h,$c,256)
[void][WI]::GetWindowTextW($h,$t,256)
$procId=0
[void][WI]::GetWindowThreadProcessId($h,[ref]$procId)
$proc = Get-Process -Id ([int]$procId) -ErrorAction SilentlyContinue
$name = if($proc){ $proc.ProcessName } else { '?' }
Write-Output ("hwnd={0} class={1} title={2} pid={3} proc={4} vis={5} iconic={6}" -f $Hwnd,$c.ToString(),$t.ToString(),$procId,$name,[WI]::IsWindowVisible($h),[WI]::IsIconic($h))
