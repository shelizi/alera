$ErrorActionPreference='Stop'
Add-Type @'
using System;using System.Runtime.InteropServices;
public static class KeySt {
  [DllImport("user32.dll")] public static extern short GetAsyncKeyState(int vk);
  [DllImport("user32.dll")] public static extern bool GetKeyboardState(byte[] s);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f,uint dx,uint dy,uint d,UIntPtr e);
}
'@
$l=[KeySt]::GetAsyncKeyState(0x01)
$r=[KeySt]::GetAsyncKeyState(0x02)
$m=[KeySt]::GetAsyncKeyState(0x04)
"LBUTTON=$l RBUTTON=$r MBUTTON=$m"
"LBUTTON_DOWN=$(($l -band 0x8000) -ne 0) RBUTTON_DOWN=$(($r -band 0x8000) -ne 0) MBUTTON_DOWN=$(($m -band 0x8000) -ne 0)"
