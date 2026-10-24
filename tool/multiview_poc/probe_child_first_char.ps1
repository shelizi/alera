param(
  [Parameter(Mandatory=$true)][ValidateRange(1,2147483647)][int]$TargetPid,
  [string]$ChildTitle='Alera - E2E Terminal',
  [ValidateRange(0,20000)][int]$DelayMs=0,
  [string]$Text='echo traceprobe',
  [switch]$NoEnter,
  [switch]$Click
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
public static class AleraE2EFirstChar {
  public delegate bool Callback(IntPtr hwnd,IntPtr ignored);
  [DllImport("user32.dll")] public static extern bool EnumWindows(Callback cb,IntPtr unused);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd,out uint pid);
  [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hwnd,StringBuilder buf,int max);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hwnd,int cmd);
  [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint src,uint dest,bool attach);
  [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x,int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint flags,uint dx,uint dy,uint data,UIntPtr extra);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd,out RECT rect);
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr hwnd,out RECT rect);
  [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr hwnd,ref POINT pt);
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X; public int Y; }
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
  public static void ClickCenter(IntPtr hwnd) {
    RECT rc;GetClientRect(hwnd,out rc);
    POINT pt;pt.X=(rc.Right-rc.Left)/2;pt.Y=(rc.Bottom-rc.Top)/2;
    ClientToScreen(hwnd,ref pt);
    SetCursorPos(pt.X,pt.Y);
    mouse_event(0x0002,0,0,0,UIntPtr.Zero);
    mouse_event(0x0004,0,0,0,UIntPtr.Zero);
  }
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
      if(buf.ToString().StartsWith(title)){found=hwnd;return false;}
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
  public static uint TypeScanCodes(ushort[] scans,bool enter) {
    int n=scans.Length*2+(enter?2:0);
    var items=new INPUT[n];
    int j=0;
    foreach(var scan in scans){
      items[j].type=1;items[j].union.key.scan=scan;
      items[j].union.key.vk=0;items[j].union.key.flags=8;
      j++;
      items[j].type=1;items[j].union.key.scan=scan;
      items[j].union.key.vk=0;items[j].union.key.flags=10;
      j++;
    }
    if(enter){
      items[j].type=1;items[j].union.key.vk=0x0D;items[j].union.key.scan=0x1C;items[j].union.key.flags=0;j++;
      items[j].type=1;items[j].union.key.vk=0x0D;items[j].union.key.scan=0x1C;items[j].union.key.flags=2;j++;
    }
    return SendInput((uint)items.Length,items,Marshal.SizeOf<INPUT>());
  }
}
'@
$sw=[System.Diagnostics.Stopwatch]::StartNew()
$child=[IntPtr]::Zero
$deadline=[DateTime]::UtcNow.AddSeconds(15)
do {
  $child=[AleraE2EFirstChar]::Find($app.Id,$ChildTitle)
  if($child -ne [IntPtr]::Zero){break}
  Start-Sleep -Milliseconds 40
} while([DateTime]::UtcNow -lt $deadline)
if($child -eq [IntPtr]::Zero){throw "Child window '$ChildTitle' did not appear"}
$foundMs=$sw.ElapsedMilliseconds
if($DelayMs -gt 0){Start-Sleep -Milliseconds $DelayMs}
$prior=[AleraE2EFirstChar]::GetForegroundWindow()
$focusMs=$sw.ElapsedMilliseconds
if(-not [AleraE2EFirstChar]::Focus($child)){
  throw 'Windows foreground policy rejected focusing isolated E2E child'
}
if($Click){
  [AleraE2EFirstChar]::ClickCenter($child)
  Start-Sleep -Milliseconds 60
}
$map=@{
  ' '=0x39;'a'=0x1E;'b'=0x30;'c'=0x2E;'d'=0x20;'e'=0x12;'f'=0x21;
  'g'=0x22;'h'=0x23;'i'=0x17;'j'=0x24;'k'=0x25;'l'=0x26;'m'=0x32;
  'n'=0x31;'o'=0x18;'p'=0x19;'q'=0x10;'r'=0x13;'s'=0x1F;'t'=0x14;
  'u'=0x16;'v'=0x2F;'w'=0x11;'x'=0x2D;'y'=0x15;'z'=0x2C;
  '0'=0x0B;'1'=0x02;'2'=0x03;'3'=0x04;'4'=0x05;'5'=0x06;'6'=0x07;
  '7'=0x08;'8'=0x09;'9'=0x0A;'-'=0x0C;'_'=0x0C;'.'=0x34;'/'=0x35;
}
$scans=@()
foreach($ch in $Text.ToLowerInvariant().ToCharArray()){
  if(-not $map.ContainsKey([string]$ch)){throw "No scan code for '$ch'"}
  $scans+=[ushort]$map[[string]$ch]
}
$beforeMs=$sw.ElapsedMilliseconds
$sent=[AleraE2EFirstChar]::TypeScanCodes([ushort[]]$scans,-not $NoEnter)
Write-Output "FIRST_CHAR_SENT events=$sent foundMs=$foundMs focusedMs=$focusMs sentAtMs=$beforeMs child=$child text='$Text'"
if($prior -ne [IntPtr]::Zero){
  Start-Sleep -Milliseconds 150
  [void][AleraE2EFirstChar]::Focus($prior)
}
