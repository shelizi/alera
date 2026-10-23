param(
 [ValidateSet("inspect", "click", "rightClick", "type", "close")][string]$Mode="inspect",
 [int]$X=0,
 [int]$Y=0,
 [int]$TargetPid=0,
 [string]$WindowTitle='',
 [string]$Text='',
 [ValidateSet('Debug','Release')][string]$Configuration='Debug'
)
$ErrorActionPreference="Stop"
$root=(Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$buildPath = if($Configuration -eq 'Release') {
 'build/windows/shared_release_ninja/runner/Release/alera-dev.exe'
} else { 'build/windows/shared_gui_ninja/runner/Debug/alera-dev.exe' }
$expectedExe=Join-Path $root $buildPath
$apps=@(Get-Process alera-dev -ErrorAction SilentlyContinue |
 Where-Object {$_.Path -eq $expectedExe -and ($TargetPid -eq 0 -or $_.Id -eq $TargetPid)})
if($apps.Count -ne 1){throw "Expected exactly one isolated E2E Alera; found $($apps.Count)"}
$app=$apps[0]
if($app.MainWindowHandle -eq 0){throw 'No test HWND'}
Add-Type @'
using System;
using System.Collections.Generic;
using System.Text;
using System.Runtime.InteropServices;
public static class AleraE2EUI {
 public delegate bool Callback(IntPtr hwnd, IntPtr ignored);
 [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left,Top,Right,Bottom; }
 [DllImport("user32.dll")] public static extern bool EnumWindows(Callback callback,IntPtr value);
 [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr parent,Callback callback,IntPtr value);
 [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd,out RECT rect);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr hwnd,StringBuilder name,int len);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hwnd,StringBuilder text,int max);
 [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hwnd,uint msg,UIntPtr wparam,IntPtr lparam);
 [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd,out uint pid);
 public static string ClassName(IntPtr hwnd){
   var buf=new StringBuilder(260);
   GetClassName(hwnd,buf,buf.Capacity);
   return buf.ToString();
 }
 public static IntPtr[] Children(IntPtr window){
   var items=new List<IntPtr>();
   EnumChildWindows(window,(h,_)=>{items.Add(h);return true;},IntPtr.Zero);
   return items.ToArray();
 }
 public static IntPtr Find(int pid,string title){
   IntPtr match=IntPtr.Zero;
   EnumWindows((hwnd,_)=>{
     uint owner; GetWindowThreadProcessId(hwnd,out owner);
     if(owner != (uint)pid)return true;
     var sb=new StringBuilder(260); GetWindowText(hwnd,sb,sb.Capacity);
     if(sb.ToString()==title){match=hwnd;return false;}
     return true;
   },IntPtr.Zero);
   return match;
 }
 public static IntPtr Pack(int x,int y){
   return new IntPtr((y & 0xffff)<<16 | (x & 0xffff));
 }
}
'@
$parent=$app.MainWindowHandle
if($WindowTitle -ne ''){
 $parent=[AleraE2EUI]::Find($app.Id,$WindowTitle)
 if($parent -eq [IntPtr]::Zero){throw "E2E app does not own exact window title: $WindowTitle"}
}
$pRect=New-Object AleraE2EUI+RECT
[void][AleraE2EUI]::GetWindowRect($parent,[ref]$pRect)
Write-Output "PID=$($app.Id) MAIN=$parent BOUNDS=$($pRect.Left),$($pRect.Top),$($pRect.Right),$($pRect.Bottom)"
$kids=@([AleraE2EUI]::Children($parent))
foreach($kid in $kids){
 $r=New-Object AleraE2EUI+RECT
 [void][AleraE2EUI]::GetWindowRect($kid,[ref]$r)
 Write-Output "CHILD=$kid CLASS=$([AleraE2EUI]::ClassName($kid)) BOUNDS=$($r.Left),$($r.Top),$($r.Right),$($r.Bottom) VISIBLE=$([AleraE2EUI]::IsWindowVisible($kid))"
}
if($Mode -eq 'inspect'){exit 0}
if($Mode -eq 'close'){
 $ok=[AleraE2EUI]::PostMessage($parent,0x10,[UIntPtr]::Zero,[IntPtr]::Zero)
 Write-Output "WM_CLOSE_POSTED=$ok"
 exit 0
}
$views=@($kids|Where-Object {[AleraE2EUI]::ClassName($_) -like '*FLUTTER*'})
if($views.Count -ne 1){throw "Expected 1 Flutter native child HWND, found $($views.Count)"}
$view=$views[0]
if($Mode -eq 'type'){
 if($WindowTitle -eq '' -or $Text.Length -lt 1 -or $Text.Length -gt 128){throw 'Typing permitted only for explicit isolated child and 1..128 characters'}
 $sent=0
 foreach($ch in $Text.ToCharArray()){
   if([AleraE2EUI]::PostMessage($view,0x0102,[UIntPtr][int][char]$ch,[IntPtr]::Zero)){$sent+=1}
 }
 Write-Output "POSTED_CHILD_WM_CHAR=$sent VIEW=$view TITLE=$WindowTitle"
 exit 0
}
$rect=New-Object AleraE2EUI+RECT
[void][AleraE2EUI]::GetWindowRect($view,[ref]$rect)
$cx=$X-($rect.Left-$pRect.Left)
$cy=$Y-($rect.Top-$pRect.Top)
if($cx -lt 0 -or $cy -lt 0 -or $cx -ge ($rect.Right-$rect.Left) -or $cy -ge ($rect.Bottom-$rect.Top)){
 throw "Click outside Flutter view: $cx,$cy"
}
$lp=[AleraE2EUI]::Pack($cx,$cy)
$move=[AleraE2EUI]::PostMessage($view,0x200,[UIntPtr]::Zero,$lp)
if($Mode -eq 'rightClick'){
 $down=[AleraE2EUI]::PostMessage($view,0x204,[UIntPtr]2,$lp)
 $up=[AleraE2EUI]::PostMessage($view,0x205,[UIntPtr]::Zero,$lp)
}else{
 $down=[AleraE2EUI]::PostMessage($view,0x201,[UIntPtr]1,$lp)
 $up=[AleraE2EUI]::PostMessage($view,0x202,[UIntPtr]::Zero,$lp)
}
Write-Output "POSTED_FLUTTER_$Mode=$move/$down/$up view=$view client=$cx,$cy window_relative=$X,$Y"
