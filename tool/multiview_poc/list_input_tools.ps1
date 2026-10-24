$pat = 'gesture|mouse|powertoys|stroke|wg$|keyman|hook|sharex|autohotkey|ahk|displayfusion|actualtools|stardock|windowblinds|winxcorners|bugn|overlay|inject'
Get-Process | Where-Object { $_.Name -match $pat -or $_.MainWindowTitle -match $pat } |
  Select-Object Id, Name, MainWindowTitle | Format-Table -AutoSize
"--- all non-ms user processes ---"
Get-Process | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object Id, Name, MainWindowTitle | Sort-Object Name | Format-Table -AutoSize
