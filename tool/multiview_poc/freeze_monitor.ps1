param(
  [Parameter(Mandatory=$true)][ValidateRange(1,2147483647)][int]$AppPid,
  [Parameter(Mandatory=$true)][ValidateRange(1,2147483647)][int]$HostPid,
  [Parameter(Mandatory=$true)][long]$AppHwnd,
  [int]$MonitorSec = 150,
  [int]$PostResumeSec = 45,
  [int]$HostPort = 5635,
  [string]$OutFile = "freeze_monitor_out.csv"
)
$ErrorActionPreference='Stop'
$root=(Resolve-Path (Join-Path $PSScriptRoot '../..')).Path

Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class AleraFreezeMon {
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern uint SendInput(uint n, INPUT[] p, int cb);
  [StructLayout(LayoutKind.Sequential)] public struct KEY { public ushort vk, scan; public uint flags, time; public UIntPtr extra; }
  [StructLayout(LayoutKind.Explicit, Size=40)] public struct INPUT { [FieldOffset(0)] public uint type; [FieldOffset(8)] public KEY key; }
  public static void SendScan(IntPtr h, ushort scan) {
    var items = new INPUT[2];
    items[0].type=1; items[0].key.scan=scan; items[0].key.flags=8;
    items[1].type=1; items[1].key.scan=scan; items[1].key.flags=10;
    SetForegroundWindow(h);
    System.Threading.Thread.Sleep(250);
    SendInput(2,items,System.Runtime.InteropServices.Marshal.SizeOf(typeof(INPUT)));
  }
}
'@

$rows = New-Object System.Collections.ArrayList
$seen = @{}
function Snapshot([string]$phase){
  $conns = @(Get-NetTCPConnection -OwningProcess $AppPid -RemotePort $HostPort -ErrorAction SilentlyContinue)
  $est = @($conns | Where-Object { $_.State -eq 'Established' })
  foreach($c in $conns){ $seen[$c.LocalPort] = $true }
  [void]$rows.Add([pscustomobject]@{
    ts=(Get-Date).ToUniversalTime().ToString('HH:mm:ss.fff')
    phase=$phase
    total=$conns.Count
    established=$est.Count
    localports=($conns | ForEach-Object { $_.LocalPort } | Sort-Object) -join '|'
    states=($conns | ForEach-Object { $_.State } | Group-Object | ForEach-Object { "$($_.Name)x$($_.Count)" }) -join '|'
  })
}

"PHASE=baseline"; Snapshot 'baseline'
& pwsh -NoProfile -File "$root/tool/multiview_poc/host_freeze_resume.ps1" -HostPid $HostPid -Action Freeze
$frozenAt = Get-Date
"FROZEN_AT=$($frozenAt.ToUniversalTime().ToString('HH:mm:ss'))"

# Trigger a request so request-timeout -> heartbeat -> disconnect fires.
[AleraFreezeMon]::SendScan([IntPtr]$AppHwnd,[ushort]30)
Start-Sleep -Milliseconds 150
[AleraFreezeMon]::SendScan([IntPtr]$AppHwnd,[ushort]28)
"KEY_SENT"

$deadline = $frozenAt.AddSeconds($MonitorSec)
while((Get-Date) -lt $deadline){ Snapshot 'frozen'; Start-Sleep -Seconds 2 }

& pwsh -NoProfile -File "$root/tool/multiview_poc/host_freeze_resume.ps1" -HostPid $HostPid -Action Resume
$resumedAt = Get-Date
"RESUMED_AT=$($resumedAt.ToUniversalTime().ToString('HH:mm:ss'))"

$deadline2 = $resumedAt.AddSeconds($PostResumeSec)
while((Get-Date) -lt $deadline2){ Snapshot 'resumed'; Start-Sleep -Seconds 2 }
Snapshot 'final'

$rows | Export-Csv -Path $OutFile -NoTypeInformation
"UNIQUE_LOCAL_PORTS=$($seen.Count)"
$rows | Select-Object -Last 15 | Format-Table -AutoSize
"MONITOR_DONE OutFile=$OutFile"
