param(
 [ValidateRange(1,200)][int]$Cycles=20,
 [ValidateRange(0,20)][int]$NativeCloseEvery=5,
 [ValidateRange(50,2000)][int]$ChildReadyWaitMs=350,
 [int]$TabX=477,
 [int]$TabY=65,
 [int]$MenuX=615,
 [int]$MenuY=252,
 [int]$ReturnX=1277,
 [int]$ReturnY=74,
 [ValidateRange(1,4)][int]$MaxGestureAttempts=3,
 [ValidatePattern('^Alera - [A-Za-z0-9_ .-]{1,90}$')][string]$ChildTitle='Alera - handoff_test.dart',
 [ValidatePattern('^[A-Za-z0-9_-]+\.json$')][string]$ReportName='real_editor_native_cycles.json',
 [ValidateSet('EDITOR','TERMINAL')][string]$Scenario='EDITOR',
 [switch]$PreflightOnly,
 [switch]$NativeMenuClick
)
$ErrorActionPreference='Stop'
$base=(Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$exe=Join-Path $base 'build/windows/shared_gui_ninja/runner/Debug/alera-dev.exe'
$buildConfig=Join-Path $base 'windows/flutter/ephemeral/generated_config.cmake'
if($Scenario -eq 'TERMINAL') {
  if(-not $PSBoundParameters.ContainsKey('MenuX')){$MenuX=400}
  if(-not $PSBoundParameters.ContainsKey('MenuY')){$MenuY=268}
  # Terminal is intentionally disabled in standard Alera builds. Avoid
  # mistaking process-backed windows for this experimental one-engine test.
  $define=[Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes('ALERA_EXPERIMENTAL_SHARED_TERMINAL_TABS=true'))
  if(-not (Test-Path $buildConfig) -or -not [IO.File]::ReadAllText($buildConfig).Contains($define)) {
    throw 'Refusing Terminal stress test without compiled experimental Terminal flag'
  }
  # Win32 PostMessage can open and native-close this tab reliably. The Return
  # icon requires real desktop input, verified separately. Do not turn a
  # synthetic hit-test failure into an Alera product failure.
  if($NativeCloseEvery -ne 1) {
    throw 'Terminal synthetic-cycle probe requires -NativeCloseEvery 1; verify Return using real mouse input'
  }
}
$procs=@(Get-Process alera-dev -ErrorAction SilentlyContinue | Where-Object Path -eq $exe)
if($NativeMenuClick -and $Scenario -ne 'TERMINAL'){
  throw 'NativeMenuClick is only calibrated for the isolated E2E Terminal context menu'
}
if($procs.Count -ne 1){throw "Expected exactly one E2E Alera process; found $($procs.Count)"}
$app=$procs[0]
$pidToProbe=$app.Id
Add-Type @'
using System;
using System.Collections.Generic;
using System.Text;
using System.Runtime.InteropServices;
public static class AleraE2ECyclesNative {
 public delegate bool Callback(IntPtr handle, IntPtr unused);
 [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left,Top,Right,Bottom; }
 [DllImport("user32.dll")] public static extern bool EnumWindows(Callback cb,IntPtr data);
 [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr hwnd, Callback cb,IntPtr data);
 [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
 [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd,out uint pid);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hwnd,StringBuilder outBuf,int size);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr hwnd,StringBuilder outBuf,int size);
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd,out RECT rect);
 [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hwnd,uint msg,UIntPtr wp,IntPtr lp);
 [DllImport("user32.dll")] public static extern uint GetGuiResources(IntPtr proc,uint flag);
 public static IntPtr FindTopLevel(int targetPid,string title) {
   IntPtr found=IntPtr.Zero;
   EnumWindows((h,_)=>{
     uint pid;GetWindowThreadProcessId(h,out pid);
     if(pid!=(uint)targetPid || !IsWindowVisible(h)) return true;
     var buf=new StringBuilder(256);GetWindowText(h,buf,buf.Capacity);
     if(buf.ToString()==title){found=h;return false;}
     return true;
   },IntPtr.Zero);
   return found;
 }
 public static int OwnedTopLevelCount(int targetPid){
   var count=0;
   EnumWindows((h,_)=>{uint pid;GetWindowThreadProcessId(h,out pid);
     if(pid==(uint)targetPid && IsWindowVisible(h)) count++;return true;},IntPtr.Zero);
   return count;
 }
 public static IntPtr FlutterChild(IntPtr parent) {
   IntPtr found=IntPtr.Zero;
   EnumChildWindows(parent,(h,_)=>{
     var b=new StringBuilder(256);GetClassName(h,b,b.Capacity);
     if(b.ToString()=="FLUTTERVIEW"){found=h;return false;}
     return true;
   },IntPtr.Zero);
   return found;
 }
 public static IntPtr Pack(int x,int y){
   return new IntPtr((y & 0xffff)<<16 | (x & 0xffff));
 }
 public static bool Mouse(IntPtr parent,int x,int y,bool right){
   if(parent==IntPtr.Zero)return false;
   var view=FlutterChild(parent);
   if(view==IntPtr.Zero)return false;
   RECT pr,vr;
   GetWindowRect(parent,out pr);GetWindowRect(view,out vr);
   int cx=x-(vr.Left-pr.Left);
   int cy=y-(vr.Top-pr.Top);
   if(cx<0 || cy<0 || cx>=vr.Right-vr.Left || cy>=vr.Bottom-vr.Top)return false;
   var lp=Pack(cx,cy);
   uint flag=right?2u:1u;
   uint down=right?0x204u:0x201u;
   uint up=right?0x205u:0x202u;
   return PostMessage(view,0x200,UIntPtr.Zero,lp)
     && PostMessage(view,down,(UIntPtr)flag,lp)
     && PostMessage(view,up,UIntPtr.Zero,lp);
 }
}
'@
$mainTitle='Alera Dev'
$childTitle=$ChildTitle
$main=[AleraE2ECyclesNative]::FindTopLevel($pidToProbe,$mainTitle)
if($main -eq [IntPtr]::Zero){throw "E2E main not visible"}
if([AleraE2ECyclesNative]::FindTopLevel($pidToProbe,$childTitle) -ne [IntPtr]::Zero){throw 'Unexpected preexisting child'}
if([AleraE2ECyclesNative]::OwnedTopLevelCount($pidToProbe) -ne 1){throw 'Unexpected extra E2E HWNDs'}
function Assert-PointInsideE2EFlutterView([IntPtr]$window,[int]$x,[int]$y,[string]$label){
  $view=[AleraE2ECyclesNative]::FlutterChild($window)
  if($view -eq [IntPtr]::Zero){throw 'No E2E FlutterView to hit-test'}
  $parentRect=New-Object AleraE2ECyclesNative+RECT
  $viewRect=New-Object AleraE2ECyclesNative+RECT
  if(-not [AleraE2ECyclesNative]::GetWindowRect($window,[ref]$parentRect) -or
      -not [AleraE2ECyclesNative]::GetWindowRect($view,[ref]$viewRect)) {
    throw 'Could not read E2E Win32 window bounds'
  }
  $clientX=$x-($viewRect.Left-$parentRect.Left)
  $clientY=$y-($viewRect.Top-$parentRect.Top)
  if($clientX -lt 0 -or $clientY -lt 0 -or
      $clientX -ge ($viewRect.Right-$viewRect.Left) -or
      $clientY -ge ($viewRect.Bottom-$viewRect.Top)) {
    throw "$label coordinate ($x,$y) outside E2E FlutterView; viewport may have resized. Recalibrate before starting a stream."
  }
}
Assert-PointInsideE2EFlutterView $main $TabX $TabY 'Tab'
Assert-PointInsideE2EFlutterView $main $MenuX $MenuY 'Popup menu'
if($PreflightOnly){
  Write-Output "E2E_COORDINATES_VALID=1 PID=$pidToProbe SCENARIO=$Scenario"
  exit 0
}
$sw=[System.Diagnostics.Stopwatch]::StartNew()
$cyclesResults=New-Object 'System.Collections.Generic.List[object]'
$beginRss=[math]::Round($app.WorkingSet64/1MB,1)
$beginHandles=$app.HandleCount
$beginGdi=[AleraE2ECyclesNative]::GetGuiResources($app.Handle,0)
try{
 for($i=1;$i -le $Cycles;$i++){
   $app.Refresh()
   if($app.HasExited){throw "Test Alera exited unexpectedly at cycle $i"}
   $main=[AleraE2ECyclesNative]::FindTopLevel($pidToProbe,$mainTitle)
   if($main -eq [IntPtr]::Zero){throw "Cycle $i main HWND missing"}
   $started=$sw.ElapsedMilliseconds
   $child=[IntPtr]::Zero
   $attempts=0
   do {
     $attempts++
     if(-not [AleraE2ECyclesNative]::Mouse($main,$TabX,$TabY,$true)){throw "Cycle $i attempt $attempts right click failed"}
     # Popup menus are async frame-scheduled: never click immediately after
     # synthetic WM_RBUTTONUP, especially on high-DPI monitors.
     Start-Sleep -Milliseconds 500
     if($NativeMenuClick){
       # The Win32 synthetic click can be accepted without selecting a Flutter
       # popup row after crossing DPI boundaries; target the actual HWND with
       # per-monitor physical coordinates and fail closed on ownership changes.
       & pwsh -NoProfile -File (Join-Path $PSScriptRoot 'probe_isolated_e2e_mouse.ps1') `
         -Mode click -X $MenuX -Y $MenuY -TargetPid $pidToProbe -Configuration Debug
       if($LASTEXITCODE -ne 0){throw "Cycle $i attempt $attempts guarded real menu click failed"}
     }elseif(-not [AleraE2ECyclesNative]::Mouse($main,$MenuX,$MenuY,$false)){
       throw "Cycle $i attempt $attempts synthetic menu click failed"
     }
     $end=[DateTime]::UtcNow.AddSeconds(3)
     do {
       $child=[AleraE2ECyclesNative]::FindTopLevel($pidToProbe,$childTitle)
       if($child -ne [IntPtr]::Zero){break}
       Start-Sleep -Milliseconds 50
     }while([DateTime]::UtcNow -lt $end)
     if($child -eq [IntPtr]::Zero -and $attempts -lt $MaxGestureAttempts) {
       Write-Output "GESTURE_RETRY_CYCLE=$i AFTER_ATTEMPT=$attempts"
       Start-Sleep -Milliseconds 850
     }
   }while($child -eq [IntPtr]::Zero -and $attempts -lt $MaxGestureAttempts)
   if($child -eq [IntPtr]::Zero){throw "Cycle $i did not create child native window after $attempts attempts"}
   $openMs=$sw.ElapsedMilliseconds-$started
   Start-Sleep -Milliseconds $ChildReadyWaitMs
   if([AleraE2ECyclesNative]::OwnedTopLevelCount($pidToProbe) -ne 2){throw "Cycle $i wrong child+main count"}
   $nativeClose=$NativeCloseEvery -gt 0 -and ($i % $NativeCloseEvery -eq 0)
   $returnStart=$sw.ElapsedMilliseconds
   if($nativeClose) {
     if(-not [AleraE2ECyclesNative]::PostMessage($child,0x10,[UIntPtr]::Zero,[IntPtr]::Zero)){
       throw "Cycle $i native WM_CLOSE failed"
     }
   }else{
     if(-not [AleraE2ECyclesNative]::Mouse($child,$ReturnX,$ReturnY,$false)){
       throw "Cycle $i Return-tab icon click failed"
     }
   }
   $closed=$false
   $end=[DateTime]::UtcNow.AddSeconds(10)
   do{
     if([AleraE2ECyclesNative]::FindTopLevel($pidToProbe,$childTitle) -eq [IntPtr]::Zero){
       $closed=$true
       break
     }
     Start-Sleep -Milliseconds 50
   }while([DateTime]::UtcNow -lt $end)
   if(-not $closed){throw "Cycle $i child window did not close"}
   $closeMs=$sw.ElapsedMilliseconds-$returnStart
   # Native child HWND removal precedes the Dart source-tab frame barrier.
   # Allow the main tab and its gesture detector to remount before
   # triggering the next simulated right-click.
   Start-Sleep -Milliseconds 900
   if([AleraE2ECyclesNative]::OwnedTopLevelCount($pidToProbe) -ne 1){throw "Cycle $i leaked HWND"}
   $app.Refresh()
   $row=[pscustomobject]@{
     cycle=$i;method=$(if($nativeClose){'WM_CLOSE'}else{'Return'});
     nativeGestureAttempts=$attempts;
     openedMs=$openMs;closedMs=$closeMs;
     workingSetMiB=[math]::Round($app.WorkingSet64/1MB,1);
     handles=$app.HandleCount;
     gdi=[AleraE2ECyclesNative]::GetGuiResources($app.Handle,0)
   }
   $cyclesResults.Add($row)
   Write-Output "CYCLE=$i MODE=$($row.method) GESTURE_ATTEMPTS=$attempts OPEN_MS=$($row.openedMs) RETURN_MS=$($row.closedMs) RSS_MIB=$($row.workingSetMiB) HANDLES=$($row.handles) GDI=$($row.gdi)"
 }
}finally {
 $folder=Join-Path $base 'build/windows/shared_gui_ninja/diagnostics'
 New-Item -ItemType Directory -Force -Path $folder|Out-Null
 $file=Join-Path $folder $ReportName
 $payload=[pscustomobject]@{
   scenario=$Scenario;childTitle=$childTitle;
   pid=$pidToProbe;requested=$Cycles;completed=$cyclesResults.Count;
   rssInitialMiB=$beginRss;handlesInitial=$beginHandles;gdiInitial=$beginGdi;
   results=$cyclesResults.ToArray()
 }
 $payload|ConvertTo-Json -Depth 6 | Set-Content -Path $file -Encoding utf8
 Write-Output "REPORT=$file COMPLETED=$($cyclesResults.Count)/$Cycles"
}
if($cyclesResults.Count -ne $Cycles){exit 2}
$opens=@($cyclesResults|ForEach-Object openedMs|Sort-Object)
$closes=@($cyclesResults|ForEach-Object closedMs|Sort-Object)
$ix=[math]::Max(0,[math]::Ceiling($Cycles*0.95)-1)
Write-Output "E2E_${Scenario}_STRESS_PASS=$Cycles/$Cycles OPEN_P95_MS=$($opens[$ix]) CLOSE_P95_MS=$($closes[$ix])"
