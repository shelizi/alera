Add-Type @'
using System;using System.Runtime.InteropServices;
public static class Monitors {
  public delegate bool EnumMon(IntPtr h, IntPtr dc, IntPtr r, IntPtr l);
  [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
  [DllImport("user32.dll")] public static extern bool EnumDisplayMonitors(IntPtr dc, IntPtr clip, EnumMon cb, IntPtr l);
  [DllImport("user32.dll")] public static extern bool GetMonitorInfoW(IntPtr m, ref MI mi);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L,T,R,B; }
  [StructLayout(LayoutKind.Sequential)] public struct MI { public int cbSize; public RECT monitor, work; public uint flags; }
}
'@
[void][Monitors]::SetThreadDpiAwarenessContext([IntPtr]::New(-4))
$cb = [Monitors+EnumMon]{ param($h,$dc,$r,$l)
  $mi = New-Object Monitors+MI; $mi.cbSize = [Runtime.InteropServices.Marshal]::SizeOf([type][Monitors+MI])
  [void][Monitors]::GetMonitorInfoW($h,[ref]$mi)
  "monitor phys=$($mi.monitor.L),$($mi.monitor.T)-$($mi.monitor.R),$($mi.monitor.B) work=$($mi.work.L),$($mi.work.T)-$($mi.work.R),$($mi.work.B) flags=$($mi.flags)"
  return $true
}
[void][Monitors]::EnumDisplayMonitors([IntPtr]::Zero,[IntPtr]::Zero,$cb,[IntPtr]::Zero)
