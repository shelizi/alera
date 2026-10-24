$ErrorActionPreference='Stop'
Add-Type @'
using System;using System.Text;using System.Runtime.InteropServices;
public class MenuEnum {
  public delegate bool ECB(IntPtr h,IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(ECB cb,IntPtr l);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern int GetClassName(IntPtr h,StringBuilder s,int n);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h,out RECT r);
  public struct RECT { public int Left,Top,Right,Bottom; }
}
'@
$hits=@()
$cb=[MenuEnum+ECB]{ param($h,$l)
  if([MenuEnum]::IsWindowVisible($h)){
    $sb=New-Object System.Text.StringBuilder 128
    [void][MenuEnum]::GetClassName($h,$sb,128)
    $cls=$sb.ToString()
    if($cls -eq '#32768' -or $cls -like '*Popup*' -or $cls -like '*Menu*'){
      $r=New-Object MenuEnum+RECT
      [void][MenuEnum]::GetWindowRect($h,[ref]$r)
      $script:hits+=("hwnd=$h cls=$cls rect=$($r.Left),$($r.Top)-$($r.Right),$($r.Bottom)")
    }
  }
  $true
}
[void][MenuEnum]::EnumWindows($cb,[IntPtr]::Zero)
if($hits.Count -eq 0){ 'NO_MENU_WINDOWS' } else { $hits }
