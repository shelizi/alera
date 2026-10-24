param(
  [Parameter(Mandatory=$true)][ValidateRange(1,2147483647)][int]$TargetPid,
  [string]$ChildTitle='Alera - Terminal 1',
  [ValidateRange(0,30000)][int]$ExtraDelayMs=0,
  [string]$Text='echo firstok',
  [switch]$NoEnter
)
$ErrorActionPreference='Stop'
$root=(Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$exe=Join-Path $root 'build/windows/shared_gui_ninja/runner/Debug/alera-dev.exe'
$app=Get-Process -Id $TargetPid -ErrorAction Stop
if($app.Path -ne $exe) { throw 'Refusing input: PID is not the isolated E2E build' }
Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class AleraE2EFirstCharPm {
  public delegate bool Callback(IntPtr hwnd,IntPtr ignored);
  [DllImport("user32.dll")] public static extern bool EnumWindows(Callback cb,IntPtr unused);
  [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr p,Callback cb,IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd,out uint pid);
  [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hwnd,StringBuilder buf,int max);
  [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr hwnd,StringBuilder buf,int max);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
  [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr h,uint m,UIntPtr w,IntPtr l);
  public static IntPtr FindTop(int pid,string title) {
    IntPtr found=IntPtr.Zero;
    EnumWindows((hwnd,unused)=>{
      uint owner;GetWindowThreadProcessId(hwnd,out owner);
      if(owner != (uint)pid || !IsWindowVisible(hwnd))return true;
      var buf=new StringBuilder(260);GetWindowText(hwnd,buf,buf.Capacity);
      if(buf.ToString().StartsWith(title)){found=hwnd;return false;}
      return true;
    },IntPtr.Zero);
    return found;
  }
  public static IntPtr FindView(IntPtr parent) {
    IntPtr found=IntPtr.Zero;
    EnumChildWindows(parent,(hwnd,unused)=>{
      var buf=new StringBuilder(64);GetClassName(hwnd,buf,buf.Capacity);
      if(buf.ToString()=="FLUTTERVIEW"){found=hwnd;return false;}
      return true;
    },IntPtr.Zero);
    return found;
  }
  public static int SendText(IntPtr view,string text,bool enter) {
    int n=0;
    foreach(char c in text){
      if(PostMessageW(view,0x0102,(UIntPtr)c,IntPtr.Zero)) n++;
    }
    if(enter){ PostMessageW(view,0x0102,(UIntPtr)0x0D,IntPtr.Zero); n++; }
    return n;
  }
}
'@
$sw=[System.Diagnostics.Stopwatch]::StartNew()
$child=[IntPtr]::Zero
$deadline=[DateTime]::UtcNow.AddSeconds(15)
do {
  $child=[AleraE2EFirstCharPm]::FindTop($app.Id,$ChildTitle)
  if($child -ne [IntPtr]::Zero){break}
  Start-Sleep -Milliseconds 10
} while([DateTime]::UtcNow -lt $deadline)
if($child -eq [IntPtr]::Zero){throw "Child window '$ChildTitle' did not appear"}
$foundMs=$sw.ElapsedMilliseconds
$view=[IntPtr]::Zero
$viewDeadline=[DateTime]::UtcNow.AddSeconds(10)
do {
  $view=[AleraE2EFirstCharPm]::FindView($child)
  if($view -ne [IntPtr]::Zero){break}
  Start-Sleep -Milliseconds 10
} while([DateTime]::UtcNow -lt $viewDeadline)
if($view -eq [IntPtr]::Zero){throw 'Child window has no FLUTTERVIEW'}
$viewMs=$sw.ElapsedMilliseconds
if($ExtraDelayMs -gt 0){Start-Sleep -Milliseconds $ExtraDelayMs}
$sent=[AleraE2EFirstCharPm]::SendText($view,$Text,-not $NoEnter)
Write-Output "FIRST_CHAR_PM_SENT chars=$sent foundMs=$foundMs viewMs=$viewMs sentAtMs=$($sw.ElapsedMilliseconds) child=$child view=$view text='$Text'"
