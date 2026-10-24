Add-Type -AssemblyName System.Drawing
$vs = [System.Windows.Forms.SystemInformation]::VirtualScreen
$bmp = New-Object Drawing.Bitmap $vs.Width,$vs.Height
$g = [Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($vs.Left,$vs.Top,0,0,$bmp.Size)
$bmp.Save("tool\multiview_poc\desktop.png",[Drawing.Imaging.ImageFormat]::Png)
"DESKTOP $($vs.Width)x$($vs.Height) at $($vs.Left),$($vs.Top)"
