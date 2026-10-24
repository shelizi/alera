param([string]$In,[string]$Out,[int]$X,[int]$Y,[int]$W,[int]$H)
Add-Type -AssemblyName System.Drawing
$src = [Drawing.Bitmap]::FromFile($In)
$bmp = New-Object Drawing.Bitmap $W,$H
$g = [Drawing.Graphics]::FromImage($bmp)
$g.DrawImage($src,(New-Object Drawing.Rectangle 0,0,$W,$H),(New-Object Drawing.Rectangle $X,$Y,$W,$H),[Drawing.GraphicsUnit]::Pixel)
$bmp.Save($Out,[Drawing.Imaging.ImageFormat]::Png)
"CROPPED $X,$Y ${W}x${H} -> $Out"
