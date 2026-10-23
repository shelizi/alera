param(
  [ValidateSet('Debug','Release')][string]$Configuration='Debug',
  [int]$TargetPid=0
)
$ErrorActionPreference='Stop'
$root=(Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$buildPath = if($Configuration -eq 'Release') {
  'build/windows/shared_release_ninja/runner/Release/alera-dev.exe'
} else { 'build/windows/shared_gui_ninja/runner/Debug/alera-dev.exe' }
$expectedExe=Join-Path $root $buildPath
$ps=@(Get-Process alera-dev -ErrorAction SilentlyContinue |
  Where-Object { $_.Path -eq $expectedExe -and ($TargetPid -eq 0 -or $_.Id -eq $TargetPid) })
if($ps.Count -ne 1){throw "Exactly one isolated E2E Alera required. Found $($ps.Count)"}
$app=$ps[0]
Add-Type @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class AleraE2EQuitMenu {
  public delegate bool Callback(IntPtr hwnd,IntPtr unused);
  [DllImport("user32.dll")] public static extern bool EnumWindows(Callback cb,IntPtr data);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd,out uint owner);
  [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hwnd,StringBuilder text,int cap);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hwnd,uint msg,UIntPtr wp,IntPtr lp);
  public static IntPtr Find(int targetPid,string title) {
    IntPtr found=IntPtr.Zero;
    EnumWindows((h,_)=>{
      uint owner;GetWindowThreadProcessId(h,out owner);
      if(owner != (uint)targetPid)return true;
      var buf=new StringBuilder(256);GetWindowText(h,buf,buf.Capacity);
      if(buf.ToString()==title){found=h;return false;}
      return true;
    },IntPtr.Zero);
    return found;
  }
}
'@
$hwnd=[AleraE2EQuitMenu]::Find($app.Id,'Alera Dev')
if($hwnd -eq [IntPtr]::Zero){throw 'Expected exactly titled E2E primary HWND'}
if(-not [AleraE2EQuitMenu]::PostMessage($hwnd,0x111,[UIntPtr]1004,[IntPtr]::Zero)){
  throw 'Could not send isolated test App Menu Quit'
}
Write-Output "POSTED_E2E_QUIT PID=$($app.Id) HWND=$hwnd CONFIG=$Configuration"
try {
  Wait-Process -Id $app.Id -Timeout 20 -ErrorAction Stop
  Write-Output "E2E_PROCESS_EXITED=$($app.Id)"
} catch {
  Write-Output "E2E_QUIT_PENDING=$($app.Id) (could be an unsaved Editor confirmation)"
}
