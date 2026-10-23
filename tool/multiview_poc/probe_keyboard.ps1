param(
  [ValidateSet('Debug','Release')][string]$Configuration='Debug'
)
$ErrorActionPreference='Stop'
$dir=Join-Path $PSScriptRoot "build/windows/poc_ninja/runner/$Configuration"
$exe=Join-Path $dir 'alera_multiview_poc.exe'
Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class AleraKeyboardProbe {
  delegate bool WindowCallback(IntPtr hwnd, IntPtr unused);
  [DllImport("user32.dll")] static extern bool EnumWindows(WindowCallback cb,IntPtr unused);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr hwnd,out uint pid);
  [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetWindowText(IntPtr hwnd,StringBuilder text,int max);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr hwnd);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hwnd,int cmd);
  [DllImport("user32.dll")] static extern bool AttachThreadInput(uint src,uint dst,bool attach);
  [DllImport("kernel32.dll")] static extern uint GetCurrentThreadId();
  [StructLayout(LayoutKind.Sequential)]
  public struct KEYBDINPUT {
    public ushort vk;
    public ushort scan;
    public uint flags;
    public uint time;
    public UIntPtr extra;
  }
  [StructLayout(LayoutKind.Explicit,Size=32)]
  public struct INPUTUNION { [FieldOffset(0)] public KEYBDINPUT keyboard; }
  [StructLayout(LayoutKind.Sequential)]
  public struct INPUT { public uint type; public INPUTUNION union; }
  [DllImport("user32.dll",SetLastError=true)]
  static extern uint SendInput(uint count, INPUT[] inputs,int size);
  public static IntPtr Find(uint processId,string title) {
    IntPtr matched=IntPtr.Zero;
    EnumWindows((hwnd,_)=>{
      uint pid; GetWindowThreadProcessId(hwnd,out pid);
      if(pid != processId || !IsWindowVisible(hwnd)) return true;
      var sb=new StringBuilder(256); GetWindowText(hwnd,sb,sb.Capacity);
      if(sb.ToString()==title){matched=hwnd;return false;}
      return true;
    },IntPtr.Zero);
    return matched;
  }
  public static bool Focus(IntPtr target) {
    ShowWindow(target,9);
    if(SetForegroundWindow(target) && GetForegroundWindow()==target) return true;
    var alt=new INPUT[2];
    alt[0].type=1; alt[0].union.keyboard.vk=18;
    alt[1].type=1; alt[1].union.keyboard.vk=18; alt[1].union.keyboard.flags=2;
    SendInput(2,alt,Marshal.SizeOf<INPUT>());
    if(SetForegroundWindow(target) && GetForegroundWindow()==target) return true;
    uint processId;
    uint otherThread=GetWindowThreadProcessId(GetForegroundWindow(),out processId);
    uint current=GetCurrentThreadId();
    bool attached = otherThread!=0 && otherThread!=current &&
                    AttachThreadInput(current,otherThread,true);
    try { SetForegroundWindow(target); }
    finally { if(attached) AttachThreadInput(current,otherThread,false); }
    return GetForegroundWindow()==target;
  }
  public static uint UnicodeInput(string value) {
    var inputs=new INPUT[value.Length*2];
    for(int i=0;i<value.Length;i++){
      var v=(ushort)value[i];
      inputs[2*i].type=1;
      inputs[2*i].union.keyboard.scan=v;
      inputs[2*i].union.keyboard.flags=4;
      inputs[2*i+1].type=1;
      inputs[2*i+1].union.keyboard.scan=v;
      inputs[2*i+1].union.keyboard.flags=6;
    }
    return SendInput((uint)inputs.Length,inputs,Marshal.SizeOf<INPUT>());
  }
}
'@
$prior=[AleraKeyboardProbe]::GetForegroundWindow()
$report=Join-Path $dir 'poc_input_probe.json'
if(Test-Path $report){Remove-Item $report -Force}
$p=$null
try{
  $p=Start-Process -FilePath $exe -ArgumentList '--poc-input' -WorkingDirectory $dir -PassThru
  Write-Output "INPUT started PID=$($p.Id)"
  $handles=@()
  for($i=0;$i -lt 30;$i++){
    $h0=[AleraKeyboardProbe]::Find([uint32]$p.Id,'MVD_INPUT_WINDOW_0')
    $h1=[AleraKeyboardProbe]::Find([uint32]$p.Id,'MVD_INPUT_WINDOW_1')
    if($h0 -ne [IntPtr]::Zero -and $h1 -ne [IntPtr]::Zero){$handles=@($h0,$h1);break}
    Start-Sleep -Milliseconds 100
  }
  if($handles.Count -ne 2){throw "Unable to find two visible input windows"}
  # The last opened Flutter window is normally focused. Verify both
  # that already-focused view and a subsequent focus switch back to view 0.
  foreach($i in @(1,0)){
    $target=$handles[$i]
    $focused=[AleraKeyboardProbe]::Focus($target)
    Start-Sleep -Milliseconds 320
    $actual=[AleraKeyboardProbe]::GetForegroundWindow()
    $text=if($i -eq 0){'ALERAA'}else{'ALERAB'}
    $sent=0
    if($actual -eq $target){$sent=[AleraKeyboardProbe]::UnicodeInput($text)}
    Write-Output "input target=$i hwnd=$target focused=$focused active=$actual inputEvents=$sent expected=$($text.Length*2)"
    Start-Sleep -Milliseconds 240
  }
  for($i=0;$i -lt 20;$i++){if(Test-Path $report){Get-Content $report -Raw;break}; Start-Sleep -Milliseconds 400}
  if(-not(Test-Path $report)){Write-Output 'NO_INPUT_REPORT';Get-Content (Join-Path $dir 'poc_startup_trace.txt') -Tail 12;exit 3}
  $j=Get-Content $report -Raw|ConvertFrom-Json
  if(-not $j.routingPassed){Write-Output 'INPUT_ROUTING_UNCONFIRMED';exit 4}
  Write-Output 'INPUT_ROUTING_PASS'
} finally {
  if($p){$p.Refresh();if(-not $p.HasExited){Stop-Process -Id $p.Id -Force}}
  if($prior -ne [IntPtr]::Zero){[void][AleraKeyboardProbe]::SetForegroundWindow($prior)}
}
