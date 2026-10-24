$ErrorActionPreference = 'SilentlyContinue'
Get-Item 'E:\Dropbox\work\alera\alera\.worktrees\exp-shared-terminal-20261023\build\windows\x64\runner\Debug\data\flutter_assets\kernel_blob.bin' |
  Select-Object FullName, Length, LastWriteTime | Format-List
'--- CMakeCache flutter paths ---'
Select-String -Path 'C:\a_mv_20261022\CMakeCache.txt' -Pattern 'ephemeral|FLUTTER_ROOT|CMAKE_PROJECT|SOURCE_DIR' |
  ForEach-Object { $_.Line }
