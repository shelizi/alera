Add-Type @'
using System;using System.Runtime.InteropServices;
public static class MonDpi {
  public delegate bool EnumMon(IntPtr h, IntPtr dc, IntPtr r, IntPtr l);
  [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
  [DllImport("user32.dll")] public static extern bool EnumDisplayMonitors(IntPtr dc, IntPtr clip, EnumMon cb, IntPtr l);
  [DllImport("user32.dll")] public static extern bool GetMonitorInfoW(IntPtr m, ref MI mi);
  [DllImport("shcore.dll")] public static extern int GetDpiForMonitor(IntPtr m, int t, out uint x, out uint y);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L,T,R,B; }
  [StructLayout(LayoutKind.Sequential)] public struct MI { public int cbSize; public RECT monitor, work; public uint flags; }
}
'@
[void][MonDpi]::SetThreadDpiAwarenessContext([IntPtr]::New(-4))
$cb = [MonDpi+EnumMon]{ param($h,$dc,$r,$l)
  $mi = New-Object MonDpi+MI; $mi.cbSize = [Runtime.InteropServices.Marshal]::SizeOf([type][MonDpi+MI])
  [void][MonDpi]::GetMonitorInfoW($h,[ref]$mi)
  $dx=[uint32]0; $dy=[uint32]0; [void][MonDpi]::GetDpiForMonitor($h,0,[ref]$dx,[ref]$dy)
  Write-Output ("monitor phys=" + $mi.monitor.L + "," + $mi.monitor.T + "-" + $mi.monitor.R + "," + $mi.monitor.B + " dpi=" + $dx + "x" + $dy + " scale=" + [math]::Round($dx/96,2))
  return $true
}
[void][MonDpi]::EnumDisplayMonitors([IntPtr]::Zero,[IntPtr]::Zero,$cb,[IntPtr]::Zero)
