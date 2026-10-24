param(
  [Parameter(Mandatory=$true)][string]$Name,
  [Parameter(Mandatory=$true)][int]$TargetPid
)
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$root = [System.Windows.Automation.AutomationElement]::RootElement
$prop = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ProcessIdProperty, $TargetPid)
$app = $root.FindFirst([System.Windows.Automation.TreeScope]::Children, $prop)
if($app -eq $null){ throw "no app element for pid $Pid" }
function DumpTree($el, $depth){
  if($depth -gt 14){ return }
  $n = $el.Current.Name; $ct = $el.Current.ControlType.ProgrammaticName
  if($n -ne ''){ Write-Output ("{0}[{1}] '{2}'" -f ('  '*$depth), $ct, $n) }
  $c = $el.FindAll([System.Windows.Automation.TreeScope]::Children, [System.Windows.Automation.Condition]::TrueCondition)
  foreach($x in $c){ DumpTree $x ($depth+1) }
}
$btnCond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, $Name)
$btn = $app.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $btnCond)
if($btn -eq $null){
  "BUTTON_NOT_FOUND name=$Name; tree:"
  DumpTree $app 0 | Select-Object -First 80
  throw "not found"
}
$r = $btn.Current.BoundingRectangle
"BUTTON_FOUND name=$Name type=$($btn.Current.ControlType.ProgrammaticName) rect=$($r.Left),$($r.Top),$($r.Width)x$($r.Height)"
$ip = $btn.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
if($ip){ $ip.Invoke(); "INVOKE_OK" } else { "NO_INVOKE_PATTERN" }
