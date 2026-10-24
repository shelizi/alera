param(
  [Parameter(Mandatory=$true)][ValidateRange(1,65535)][int]$LocalPort,
  [Parameter(Mandatory=$true)][ValidateRange(1,65535)][int]$RemotePort,
  [int]$OwningProcess=0
)
$ErrorActionPreference='Stop'
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class AleraTcpKill {
  [StructLayout(LayoutKind.Sequential)]
  public struct MIB_TCPROW_OWNER_PID {
    public uint state;
    public uint localAddr;
    public uint localPort;
    public uint remoteAddr;
    public uint remotePort;
    public uint owningPid;
  }
  [StructLayout(LayoutKind.Sequential)]
  public struct MIB_TCPROW {
    public uint state;
    public uint localAddr;
    public uint localPort;
    public uint remoteAddr;
    public uint remotePort;
  }
  [StructLayout(LayoutKind.Sequential)]
  public struct MIB_TCPTABLE_OWNER_PID {
    public uint numEntries;
    public MIB_TCPROW_OWNER_PID first;
  }
  [DllImport("iphlpapi.dll",SetLastError=true)]
  public static extern uint GetExtendedTcpTable(IntPtr table,out uint size,bool sort,
    uint ipVersion,uint tableClass,uint reserved);
  [DllImport("iphlpapi.dll",SetLastError=true)]
  public static extern uint SetTcpEntry(IntPtr row);

  public static string KillByLocalRemote(ushort localPort,ushort remotePort,uint pid) {
    uint size=0;
    GetExtendedTcpTable(IntPtr.Zero,out size,true,2,5,0);
    var buf=Marshal.AllocHGlobal((int)size);
    try {
      uint rc=GetExtendedTcpTable(buf,out size,true,2,5,0);
      if(rc!=0)return "gettable_rc="+rc;
      int rowSize=Marshal.SizeOf<MIB_TCPROW_OWNER_PID>();
      uint count=(uint)Marshal.ReadInt32(buf);
      var lp=(uint)(((localPort & 0xFF) << 8) | ((localPort >> 8) & 0xFF));
      var rp=(uint)(((remotePort & 0xFF) << 8) | ((remotePort >> 8) & 0xFF));
      for(uint i=0;i<count;i++){
        var ptr=IntPtr.Add(buf,4+(int)(i*rowSize));
        var row=Marshal.PtrToStructure<MIB_TCPROW_OWNER_PID>(ptr);
        if(row.localPort==lp && row.remotePort==rp &&
           row.localAddr==0x0100007Fu && row.remoteAddr==0x0100007Fu &&
           (pid==0 || row.owningPid==pid)) {
          var kill=new MIB_TCPROW {
            state=12,
            localAddr=row.localAddr, localPort=row.localPort,
            remoteAddr=row.remoteAddr, remotePort=row.remotePort,
          };
          var kptr=Marshal.AllocHGlobal(Marshal.SizeOf(kill));
          try {
            Marshal.StructureToPtr(kill,kptr,false);
            uint setRc=SetTcpEntry(kptr);
            return "settcpentry="+setRc+" pid="+row.owningPid+" local="+localPort+" remote="+remotePort;
          } finally { Marshal.FreeHGlobal(kptr); }
        }
      }
      return "not_found count="+count;
    } finally { Marshal.FreeHGlobal(buf); }
  }
}
'@
$result=[AleraTcpKill]::KillByLocalRemote([uint16]$LocalPort,[uint16]$RemotePort,[uint32]$OwningProcess)
Write-Output "TCP_KILL $result"
if($result -notlike 'settcpentry=0*'){ throw "Kill failed: $result" }
