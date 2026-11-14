<#
.SYNOPSIS
  Explicit per-user Alera Native Start Menu shortcut for unpackaged WinRT Toast.
.DESCRIPTION
  Read-only by default (Check). Install requires an explicit -Mode Install and
  a real Native executable. Classic's shortcut and AppUserModelID are never
  touched. CreateFile/VerifyFile allow offline verification outside Start Menu.
#>
[CmdletBinding()]
param(
    [ValidateSet('Check', 'CreateFile', 'VerifyFile', 'Install', 'Uninstall')]
    [string]$Mode = 'Check',
    [string]$NativeExe = '',
    [string]$ShortcutPath = ''
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$appId = 'dev.leynier.alera.native'
$startMenuShortcut = Join-Path ([Environment]::GetFolderPath('StartMenu')) 'Programs\Alera Native Preview.lnk'

# The standard WScript shortcut API cannot store System.AppUserModel.ID.
# Use native ShellLink + IPropertyStore + IPersistFile, as Microsoft documents
# for unpackaged desktop Toast. Do not use Classic's identity or shortcut.
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;

namespace AleraNativeToastShortcut
{
    [ComImport, Guid("00021401-0000-0000-C000-000000000046")]
    public class ShellLinkClass {}

    [ComImport, Guid("000214F9-0000-0000-C000-000000000046"),
     InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    public interface IShellLinkW
    {
        void GetPath(IntPtr pszFile, int cch, IntPtr pfd, uint flags);
        void GetIDList(out IntPtr idList);
        void SetIDList(IntPtr idList);
        void GetDescription(IntPtr name, int cch);
        void SetDescription([MarshalAs(UnmanagedType.LPWStr)] string name);
        void GetWorkingDirectory(IntPtr directory, int cch);
        void SetWorkingDirectory([MarshalAs(UnmanagedType.LPWStr)] string directory);
        void GetArguments(IntPtr arguments, int cch);
        void SetArguments([MarshalAs(UnmanagedType.LPWStr)] string arguments);
        void GetHotkey(out short hotkey);
        void SetHotkey(short hotkey);
        void GetShowCmd(out int showCmd);
        void SetShowCmd(int showCmd);
        void GetIconLocation(IntPtr path, int cch, out int icon);
        void SetIconLocation([MarshalAs(UnmanagedType.LPWStr)] string path, int icon);
        void SetRelativePath([MarshalAs(UnmanagedType.LPWStr)] string path, uint reserved);
        void Resolve(IntPtr window, uint flags);
        void SetPath([MarshalAs(UnmanagedType.LPWStr)] string path);
    }

    [StructLayout(LayoutKind.Sequential, Pack = 4)]
    public struct PropertyKey
    {
        public Guid FormatId;
        public uint PropertyId;
        public PropertyKey(Guid formatId, uint propertyId)
        {
            FormatId = formatId;
            PropertyId = propertyId;
        }
    }

    // Native PROPVARIANT includes a 16-byte union; on x64 the struct is 24
    // bytes. Without the trailing pointer, COM writes past the struct.
    [StructLayout(LayoutKind.Sequential)]
    public struct PropVariant
    {
        public ushort VarType;
        private ushort Reserved1, Reserved2, Reserved3;
        public IntPtr StringValue;
        private IntPtr Padding;
    }

    [ComImport, Guid("886D8EEB-8CF2-4446-8D02-CDBA1DBDCF99"),
     InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    public interface IPropertyStore
    {
        void GetCount(out uint count);
        void GetAt(uint index, out PropertyKey key);
        void GetValue(ref PropertyKey key, out PropVariant value);
        void SetValue(ref PropertyKey key, ref PropVariant value);
        void Commit();
    }

    [ComImport, Guid("0000010b-0000-0000-C000-000000000046"),
     InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    public interface IPersistFile
    {
        void GetClassID(out Guid classId);
        [PreserveSig] int IsDirty();
        void Load([MarshalAs(UnmanagedType.LPWStr)] string fileName, uint mode);
        void Save([MarshalAs(UnmanagedType.LPWStr)] string fileName, [MarshalAs(UnmanagedType.Bool)] bool remember);
        void SaveCompleted([MarshalAs(UnmanagedType.LPWStr)] string fileName);
        void GetCurFile(out IntPtr fileName);
    }

    public static class NativeShortcut
    {
        static PropertyKey AppIdKey()
        {
            return new PropertyKey(
                new Guid("9F4C2855-9F79-4B39-A8D0-E1D42DE1D5F3"), 5);
        }

        [DllImport("ole32.dll")]
        static extern int PropVariantClear(ref PropVariant variant);

        public static void Create(string path, string executable, string appId)
        {
            object com = new ShellLinkClass();
            try
            {
                IShellLinkW link = (IShellLinkW)com;
                link.SetPath(executable);
                link.SetArguments("");
                link.SetDescription("Alera Native (GPUI preview)");
                IPropertyStore store = (IPropertyStore)com;
                PropertyKey key = AppIdKey();
                PropVariant value = new PropVariant();
                value.VarType = 31; // VT_LPWSTR
                value.StringValue = Marshal.StringToCoTaskMemUni(appId);
                try
                {
                    store.SetValue(ref key, ref value);
                    store.Commit();
                    // Keep the PROPVARIANT alive until the .lnk is persisted.
                    ((IPersistFile)com).Save(path, true);
                }
                finally
                {
                    Marshal.FreeCoTaskMem(value.StringValue);
                }
            }
            finally
            {
                Marshal.ReleaseComObject(com);
            }
        }

        public static string ReadAppId(string shortcut)
        {
            object com = new ShellLinkClass();
            try
            {
                ((IPersistFile)com).Load(shortcut, 0); // STGM_READ
                IPropertyStore store = (IPropertyStore)com;
                PropertyKey key = AppIdKey();
                PropVariant value;
                store.GetValue(ref key, out value);
                try
                {

                    if (value.VarType != 31 || value.StringValue == IntPtr.Zero)
                        return null;
                    return Marshal.PtrToStringUni(value.StringValue);
                }
                finally
                {
                    PropVariantClear(ref value);
                }
            }
            finally
            {
                Marshal.ReleaseComObject(com);
            }
        }
    }
}
'@ -ErrorAction Stop

function Get-VerifiedNativeShortcut([string]$Path) {
    if (-not [IO.File]::Exists($Path)) { return $false }
    $actual = [AleraNativeToastShortcut.NativeShortcut]::ReadAppId($Path)
    return [string]::Equals($actual, $appId, [StringComparison]::Ordinal)
}

if ($Mode -eq 'Check') {
    if (Get-VerifiedNativeShortcut $startMenuShortcut) {
        Write-Output "NATIVE_TOAST_SHORTCUT=registered:$startMenuShortcut"
    } else {
        Write-Output "NATIVE_TOAST_SHORTCUT=missing_or_not_native:$startMenuShortcut"
        exit 1
    }
    exit 0
}

if ($Mode -eq 'Uninstall') {
    if (-not (Get-VerifiedNativeShortcut $startMenuShortcut)) {
        throw 'Refusing to remove an unowned or missing Start Menu shortcut'
    }
    Remove-Item -LiteralPath $startMenuShortcut -Force -ErrorAction Stop
    Write-Output "NATIVE_TOAST_SHORTCUT=removed:$startMenuShortcut"
    exit 0
}

if ($Mode -eq 'VerifyFile') {
    if (-not [IO.Path]::IsPathRooted($ShortcutPath)) {
        throw 'VerifyFile requires an absolute -ShortcutPath'
    }
    if (-not (Get-VerifiedNativeShortcut $ShortcutPath)) {
        throw 'Shortcut is missing or has the wrong AppUserModelID'
    }
    Write-Output "NATIVE_TOAST_SHORTCUT=valid:$ShortcutPath"
    exit 0
}

if (-not [IO.Path]::IsPathRooted($NativeExe)) {
    throw 'CreateFile/Install require an absolute -NativeExe path'
}
$resolvedExe = [IO.Path]::GetFullPath($NativeExe)
if (-not [IO.File]::Exists($resolvedExe) -or [IO.Path]::GetExtension($resolvedExe) -ne '.exe') {
    throw 'NativeExe must be an existing .exe'
}
if ([IO.Path]::GetFileName($resolvedExe) -notin @('AleraNative.exe', 'alera-gpui-desktop.exe')) {
    throw 'Refusing to register a non-Native executable under the Native app identity'
}
if ($Mode -eq 'Install') {
    $target = $startMenuShortcut
} else {
    if (-not [IO.Path]::IsPathRooted($ShortcutPath)) {
        throw 'CreateFile requires an absolute -ShortcutPath'
    }
    $target = [IO.Path]::GetFullPath($ShortcutPath)
    if (-not $target.EndsWith('.lnk', [StringComparison]::OrdinalIgnoreCase)) {
        throw 'ShortcutPath must end with .lnk'
    }
}

if ([IO.File]::Exists($target)) {
    throw "Refusing to overwrite an existing shortcut: $target"
}
$parent = [IO.Path]::GetDirectoryName($target)
if (-not [IO.Directory]::Exists($parent)) {
    throw "Target directory is missing; refusing to create OS folders: $parent"
}
try {
    [AleraNativeToastShortcut.NativeShortcut]::Create($target, $resolvedExe, $appId)
    if (-not (Get-VerifiedNativeShortcut $target)) {
        throw 'Shortcut AppUserModelID read-back mismatch'
    }
    Write-Output "NATIVE_TOAST_SHORTCUT=registered:$target"
} catch {
    # Only remove the file created in this call, never overwrite/delete a
    # pre-existing shortcut or any Classic registration.
    if ([IO.File]::Exists($target)) {
        Remove-Item -LiteralPath $target -Force -ErrorAction SilentlyContinue
    }
    throw
}
