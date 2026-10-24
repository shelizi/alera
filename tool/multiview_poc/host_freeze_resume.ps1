param(
  [Parameter(Mandatory=$true)][ValidateRange(1,2147483647)][int]$HostPid,
  [Parameter(Mandatory=$true)][ValidateSet('Freeze','Resume')][string]$Action
)
$ErrorActionPreference='Stop'
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class AleraHostFreeze {
  [DllImport("ntdll.dll")] public static extern int NtSuspendProcess(IntPtr h);
  [DllImport("ntdll.dll")] public static extern int NtResumeProcess(IntPtr h);
  [DllImport("kernel32.dll",SetLastError=true)] public static extern IntPtr OpenProcess(uint access,bool inherit,uint pid);
  [DllImport("kernel32.dll")] public static extern bool CloseHandle(IntPtr h);
}
'@
$proc=Get-Process -Id $HostPid -ErrorAction SilentlyContinue
if(-not $proc -or $proc.ProcessName -ne 'alera'){ throw "PID $HostPid is not the terminal host (alera.exe)" }
# PROCESS_SUSPEND_RESUME
$handle=[AleraHostFreeze]::OpenProcess(0x0800,$false,[uint32]$HostPid)
if($handle -eq [IntPtr]::Zero){ throw "OpenProcess failed: $([Runtime.InteropServices.Marshal]::GetLastWin32Error())" }
try {
  if($Action -eq 'Freeze'){ $rc=[AleraHostFreeze]::NtSuspendProcess($handle) }
  else { $rc=[AleraHostFreeze]::NtResumeProcess($handle) }
  if($rc -ne 0){ throw "Nt$($Action)Process failed: 0x$($rc.ToString('X8'))" }
  Write-Output "HOST_${Action}_OK PID=$HostPid"
} finally { [void][AleraHostFreeze]::CloseHandle($handle) }
