param([Parameter(Mandatory)][string]$Bundle)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class McpSettingsWindow {
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr hwnd);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
}
'@

$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$expected = Join-Path $root 'build\windows\shared_gui_ninja\runner\Debug\alera-dev.exe'
if ([IO.Path]::GetFullPath($Bundle) -ne $expected -or -not (Test-Path -LiteralPath $expected -PathType Leaf)) {
    throw 'Only the current checkout disposable Debug Flutter bundle may be inspected.'
}
$scope = [System.Windows.Automation.TreeScope]::Descendants
$editCondition = [System.Windows.Automation.PropertyCondition]::new(
    [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
    [System.Windows.Automation.ControlType]::Edit)
$labelCondition = [System.Windows.Automation.PropertyCondition]::new(
    [System.Windows.Automation.AutomationElement]::NameProperty, 'Tab Size')
$walker = [System.Windows.Automation.TreeWalker]::ControlViewWalker
$windows = @()
foreach ($process in @(Get-Process -Name 'alera-dev' -ErrorAction SilentlyContinue)) {
    if ($process.Path -ne $expected -or $process.MainWindowHandle -eq 0) { continue }
    $hwnd = $process.MainWindowHandle
    $element = [System.Windows.Automation.AutomationElement]::FromHandle($hwnd)
    $bounds = $element.Current.BoundingRectangle
    $fields = @()
    foreach ($label in $element.FindAll($scope, $labelCondition)) {
        # Locate the smallest semantic ancestor containing the labelled field.
        # Ambiguous/missing accessibility trees deliberately fail acceptance.
        $ancestor = $label
        for ($depth = 0; $depth -lt 6; $depth++) {
            $ancestor = $walker.GetParent($ancestor)
            if ($null -eq $ancestor -or $ancestor.Equals($element)) { break }
            $edits = $ancestor.FindAll($scope, $editCondition)
            if ($edits.Count -eq 0) { continue }
            if ($edits.Count -ne 1) { throw 'Ambiguous Tab Size accessibility field.' }
            $edit = $edits[0]
            $pattern = $null
            if (-not $edit.TryGetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern, [ref]$pattern)) {
                throw 'Tab Size has no inspectable native value.'
            }
            $rect = $edit.Current.BoundingRectangle
            $labelRect = $label.Current.BoundingRectangle
            $fields += @{
                value = ([System.Windows.Automation.ValuePattern]$pattern).Current.Value
                offscreen = ($edit.Current.IsOffscreen -or $label.Current.IsOffscreen)
                contained = ($rect.Width -gt 0 -and $rect.Height -gt 0 -and
                    $bounds.Contains($rect) -and $bounds.Contains($labelRect) -and
                    $labelRect.Bottom -ge $rect.Top -and $labelRect.Top -le $rect.Bottom)
            }
            break
        }
    }
    $windows += @{
        pid = $process.Id
        hwnd = $hwnd.ToInt64()
        visible = [McpSettingsWindow]::IsWindowVisible($hwnd)
        minimized = [McpSettingsWindow]::IsIconic($hwnd)
        foreground = ([McpSettingsWindow]::GetForegroundWindow() -eq $hwnd)
        fields = @($fields)
    }
}
@{ windows = @($windows) } | ConvertTo-Json -Depth 5 -Compress
