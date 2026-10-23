param(
 [string]$WindowTitle='Alera - handoff_test.dart',
 [int]$TargetPid=0
)
$ErrorActionPreference='Stop'
$root=(Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$expectedExe=Join-Path $root 'build/windows/shared_gui_ninja/runner/Debug/alera-dev.exe'
$targets=@(Get-Process alera-dev -ErrorAction SilentlyContinue | Where-Object {
  $_.Path -eq $expectedExe -and ($TargetPid -eq 0 -or $_.Id -eq $TargetPid)
})
if($targets.Count -ne 1){throw "Expected exactly one isolated test Alera process; got $($targets.Count)"}
$app=$targets[0]
Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class AleraE2EKeyboard {
  public delegate bool Callback(IntPtr hwnd,IntPtr ignored);
  [DllImport("user32.dll")] public static extern bool EnumWindows(Callback cb,IntPtr unused);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd,out uint pid);
  [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hwnd,StringBuilder buf,int max);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hwnd,int cmd);
  [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint src,uint dest,bool attach);
  [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
  [StructLayout(LayoutKind.Sequential)] public struct KEYBDINPUT {
    public ushort vk;
    public ushort scan;
    public uint flags;
    public uint time;
    public UIntPtr extra;
  }
  [StructLayout(LayoutKind.Explicit,Size=32)] public struct INPUTUNION {
    [FieldOffset(0)] public KEYBDINPUT key;
  }
  [StructLayout(LayoutKind.Sequential)] public struct INPUT {
    public uint type;
    public INPUTUNION union;
  }
  [DllImport("user32.dll",SetLastError=true)] public static extern uint SendInput(uint count, INPUT[] inputs,int size);
  public static IntPtr Find(int pid,string title) {
    IntPtr found=IntPtr.Zero;
    EnumWindows((hwnd,unused)=>{
      uint owner;GetWindowThreadProcessId(hwnd,out owner);
      if(owner != (uint)pid)return true;
      var buf=new StringBuilder(260);GetWindowText(hwnd,buf,buf.Capacity);
      if(buf.ToString()==title){found=hwnd;return false;}
      return true;
    },IntPtr.Zero);
    return found;
  }
  public static bool Focus(IntPtr target) {
    if(target==IntPtr.Zero)return false;
    ShowWindow(target,9);
    if(SetForegroundWindow(target) && GetForegroundWindow()==target)return true;
    uint owner;
    uint foregroundThread=GetWindowThreadProcessId(GetForegroundWindow(),out owner);
    uint current=GetCurrentThreadId();
    bool attached=foregroundThread!=0 && foregroundThread!=current &&
                  AttachThreadInput(current,foregroundThread,true);
    try { SetForegroundWindow(target); }
    finally {if(attached)AttachThreadInput(current,foregroundThread,false);}
    return GetForegroundWindow()==target;
  }
  public static uint CtrlS() {
    var items=new INPUT[4];
    ushort[] vks={0xA2,0x53,0x53,0xA2};
    ushort[] scans={0x1D,0x1F,0x1F,0x1D};
    for(int i=0;i<4;i++){
      items[i].type=1;
      items[i].union.key.vk=vks[i];
      items[i].union.key.scan=scans[i];
      items[i].union.key.flags=(uint)(i>=2?2:0);
    }
    return SendInput((uint)items.Length,items,Marshal.SizeOf<INPUT>());
  }
}
'@
$child=[AleraE2EKeyboard]::Find($app.Id,$WindowTitle)
if($child -eq [IntPtr]::Zero){throw "Missing exact detached E2E editor window"}
$prior=[AleraE2EKeyboard]::GetForegroundWindow()
$focused=$false
try{
 $focused=[AleraE2EKeyboard]::Focus($child)
 if(-not $focused){throw "Windows foreground policy rejected focusing isolated E2E child; sent no keyboard events"}
 Start-Sleep -Milliseconds 100
 if([AleraE2EKeyboard]::GetForegroundWindow() -ne $child){throw "Target lost foreground; sent no keyboard events"}
 $count=[AleraE2EKeyboard]::CtrlS()
 Write-Output "E2E_NATIVE_CTRL_S_INPUT_EVENTS=$count PID=$($app.Id) CHILD=$child"
 if($count -ne 4){throw "SendInput delivered $count/4 keyboard events"}
 Start-Sleep -Milliseconds 200
} finally {
 if($prior -ne [IntPtr]::Zero -and [AleraE2EKeyboard]::GetForegroundWindow() -ne $prior){
   $restored=[AleraE2EKeyboard]::Focus($prior)
   Write-Output "PREVIOUS_FOREGROUND_RESTORED=$restored"
 }
}
