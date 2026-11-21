# Windows-only, negative native-UUID smoke test for Alera's real ConPTY Host.
# Uses a fresh Runtime + empty workspace. It NEVER resumes an existing
# conversation, writes a model prompt, or mutates a provider auth directory.
#
# Example:
#   pwsh -NoProfile -File scripts/smoke-agent-history-conpty-windows.ps1 `
#     -CliPath D:\build\alera.exe
#
# A successful run verifies:
#   1. Alera Host -> ConPTY -> shell I/O
#   2. Claude and Codex accept their native exact UUID grammar in a PTY
#   3. Both reject a deliberately absent UUID, without starting a new chat
#
# It does NOT prove successful resume of a real session or GUI rendering.

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$CliPath,
    [string]$Root = '',
    [switch]$KeepArtifacts
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if (-not $IsWindows) {
    throw 'This smoke test requires Windows ConPTY.'
}
$CliPath = (Resolve-Path -LiteralPath $CliPath -ErrorAction Stop).Path
if ([string]::IsNullOrWhiteSpace($Root)) {
    $Root = Join-Path $env:TEMP ('alera-native-resume-' + [guid]::NewGuid().ToString('N'))
}
$Root = [IO.Path]::GetFullPath($Root)
if (Test-Path -LiteralPath $Root) {
    throw "Refusing to use a preexisting test directory: $Root"
}

$runtime = Join-Path $Root 'runtime'
$project = Join-Path $Root 'project'
$workspaceId = 'native-resume-smoke-workspace'
$projectId = 'native-resume-smoke-project'
$missingUuid = '550e8400-e29b-41d4-a716-446655449999'
$launcher = $null
$hostStarted = $false

function Invoke-AleraJson {
    param([string[]]$Arguments)
    $action = @($Arguments | Where-Object {
        $_ -in @('add', 'register', 'status', 'create', 'write', 'read')
    } | Select-Object -First 1)
    $label = if ($action.Count -gt 0) { [string]$action[0] } else { [string]$Arguments[0] }
    # Terminal reads are observational and safe to retry after a transient
    # output/protocol parse failure. NEVER retry terminal create or write:
    # their first attempt may already have changed Host state.
    $maxAttempts = if ($label -eq 'read') { 2 } else { 1 }
    for ($attempt = 1; $attempt -le $maxAttempts; $attempt++) {
        # PowerShell's native-command pipeline may transform Windows-style
        # backslash escapes in long TUI JSON. Capture *raw* UTF-8 stdout via
        # .NET Process instead; use ArgumentList to avoid shell injection.
        $info = [System.Diagnostics.ProcessStartInfo]::new($CliPath)
        $info.UseShellExecute = $false
        $info.CreateNoWindow = $true
        $info.RedirectStandardOutput = $true
        $info.RedirectStandardError = $true
        $info.StandardOutputEncoding = [System.Text.Encoding]::UTF8
        $info.StandardErrorEncoding = [System.Text.Encoding]::UTF8
        $info.WorkingDirectory = (Get-Location).Path
        foreach ($arg in $Arguments) {
            $info.ArgumentList.Add($arg)
        }
        $process = [System.Diagnostics.Process]::new()
        try {
            $process.StartInfo = $info
            if (-not $process.Start()) {
                throw "Could not start Alera CLI '$label'"
            }
            $stdoutTask = $process.StandardOutput.ReadToEndAsync()
            $stderrTask = $process.StandardError.ReadToEndAsync()
            if (-not $process.WaitForExit(30000)) {
                try { $process.Kill($true) } catch { }
                throw "Alera CLI '$label' did not exit within 30 seconds"
            }
            $raw = $stdoutTask.GetAwaiter().GetResult().Trim()
            $null = $stderrTask.GetAwaiter().GetResult()
            $exitCode = $process.ExitCode
        } finally {
            $process.Dispose()
        }
        if ($exitCode -ne 0) {
            throw "Alera CLI '$label' exited with code $exitCode"
        }
        try {
            return ($raw | ConvertFrom-Json -ErrorAction Stop)
        } catch {
            $netFailure = 'not-tested'
            if ($label -eq 'read' -and $raw.StartsWith('{') -and $raw.EndsWith('}')) {
                # PowerShell's object converter can reject a valid JSON text
                # field containing provider TUI escape sequences. Confirm with
                # the strict .NET JSON parser and read ONLY the required text
                # property. Do not treat malformed JSON as a successful read.
                try {
                    $document = [System.Text.Json.JsonDocument]::Parse($raw)
                    try {
                        return @{ text = $document.RootElement.GetProperty('text').GetString() }
                    } finally {
                        $document.Dispose()
                    }
                } catch {
                    $errorDetail = ($_.Exception.Message -replace '\\s+', ' ').Trim()
                    $netFailure = $_.Exception.GetType().Name + ':' +
                        $errorDetail.Substring(0, [Math]::Min(150, $errorDetail.Length))
                    # The response is genuinely unparseable; retry reads only.
                }
            }
            if ($attempt -lt $maxAttempts) {
                Start-Sleep -Milliseconds 200
                continue
            }
            # Report shape, not terminal text or provider-auth information.
            $leading = if ($raw.Length -gt 0) { [int][char]$raw[0] } else { -1 }
            $trailing = if ($raw.Length -gt 0) { [int][char]$raw[$raw.Length - 1] } else { -1 }
            $parseCategory = $_.CategoryInfo.Reason
            throw "Alera CLI '$label' returned invalid JSON (chars=$($raw.Length), firstCharCode=$leading, lastCharCode=$trailing, reason=$parseCategory, dotnet=$netFailure)"
        }
    }
}

function New-IsolatedTerminal {
    $result = Invoke-AleraJson -Arguments @(
        'terminal', '--runtime-dir', $runtime, '--no-start', '--json',
        'create', '--workspace-id', $workspaceId
    )
    $handle = [string]$result.attachment.sessionId
    if ([string]::IsNullOrWhiteSpace($handle)) {
        throw 'Host did not provide a terminal handle'
    }
    return $handle
}

function Send-TerminalCommand {
    param([string]$Handle, [string]$Command)
    $null = Invoke-AleraJson -Arguments @(
        'terminal', '--runtime-dir', $runtime, '--no-start', '--json',
        'write', '--handle', $Handle, '--text', $Command, '--enter'
    )
}

function Read-TerminalText {
    param([string]$Handle)
    $result = Invoke-AleraJson -Arguments @(
        'terminal', '--runtime-dir', $runtime, '--no-start', '--json',
        'read', '--handle', $Handle, '--max-bytes', '32768'
    )
    return [string]$result.text
}

function Wait-TerminalMatch {
    param([string]$Handle, [string]$Expression, [int]$Seconds = 22)
    $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
    while ([DateTime]::UtcNow -lt $deadline) {
        $output = Read-TerminalText $Handle
        if ($output -match $Expression) {
            return $true
        }
        Start-Sleep -Milliseconds 500
    }
    return $false
}

try {
    $null = New-Item -ItemType Directory -Path $runtime -Force
    $null = New-Item -ItemType Directory -Path $project -Force

    $null = Invoke-AleraJson -Arguments @(
        'project', '--runtime-dir', $runtime, '--json', 'add',
        '--id', $projectId, '--name', 'Native Resume Smoke',
        '--repo-path', $project, '--kind', 'folder'
    )
    $null = Invoke-AleraJson -Arguments @(
        'workspace', '--runtime-dir', $runtime, '--json', 'register',
        '--id', $workspaceId, '--project-id', $projectId,
        '--name', 'Isolated ConPTY', '--path', $project, '--kind', 'main'
    )

    # The starter may wait for its detached child on some Windows shells.
    # Launch it independently and poll the authenticated, isolated Runtime.
    $launcher = Start-Process -FilePath $CliPath -ArgumentList @(
        'runtime', '--runtime-dir', ('"' + $runtime + '"'), '--json', 'start'
    ) -WindowStyle Hidden -PassThru
    $deadline = [DateTime]::UtcNow.AddSeconds(45)
    while ([DateTime]::UtcNow -lt $deadline) {
        try {
            $status = Invoke-AleraJson -Arguments @('runtime', '--runtime-dir', $runtime, '--json', 'status')
            if ($status.running -eq $true) {
                $hostStarted = $true
                break
            }
        } catch {
            # A detached Host may not have finished publishing its control file.
        }
        Start-Sleep -Milliseconds 500
    }
    if (-not $hostStarted) {
        throw 'Isolated Alera Host did not become ready'
    }
    Write-Output 'Isolated Host: PASS'

    $shell = New-IsolatedTerminal
    Start-Sleep -Milliseconds 650
    $marker = 'ALERA_NATIVE_CONPTY_SMOKE_OK'
    Send-TerminalCommand $shell ('echo ' + $marker)
    if (-not (Wait-TerminalMatch $shell ([regex]::Escape($marker)) 10)) {
        throw 'ConPTY shell did not echo the test marker'
    }
    # The marker must occur both in the echo command line and its output.
    $rendered = Read-TerminalText $shell
    if ([regex]::Matches($rendered, [regex]::Escape($marker)).Count -lt 2) {
        throw 'ConPTY accepted input but never executed the echo command'
    }
    Write-Output 'Windows ConPTY shell input/output: PASS'

    $codex = New-IsolatedTerminal
    Start-Sleep -Milliseconds 650
    # The ephemeral override skips the version-upgrade TUI; no config is edited.
    Send-TerminalCommand $codex (
        'codex --no-daemon -c check_for_update_on_startup=false resume ' + $missingUuid
    )
    $codexMissing = Wait-TerminalMatch $codex 'No saved session found with ID' 22
    if (-not $codexMissing) {
        throw 'Codex did not reject an absent UUID in Windows ConPTY'
    }
    Write-Output 'Codex exact UUID negative Resume: PASS'

    $claude = New-IsolatedTerminal
    Start-Sleep -Milliseconds 650
    Send-TerminalCommand $claude ('claude --resume ' + $missingUuid)
    $claudeMissing = Wait-TerminalMatch $claude 'No conversation found with session ID' 22
    if (-not $claudeMissing) {
        throw 'Claude did not reject an absent UUID in Windows ConPTY'
    }
    Write-Output 'Claude exact UUID negative Resume: PASS'
    Write-Output 'Result: PASS (negative native resume only; real-session restore unverified)'
}
finally {
    # A failed readiness poll is not proof that the detached Host never
    # started. Always probe only this newly allocated Runtime before cleanup.
    $ownedHostRunning = $hostStarted
    if (-not $ownedHostRunning -and (Test-Path -LiteralPath $runtime)) {
        try {
            $ownedHostRunning = (Invoke-AleraJson -Arguments @(
                'runtime', '--runtime-dir', $runtime, '--json', 'status'
            )).running -eq $true
        } catch {
            # An incomplete bootstrap has no usable authenticated control file.
        }
    }
    if ($ownedHostRunning) {
        try {
            $null = & $CliPath runtime --runtime-dir $runtime --json stop --force 2>&1
        } catch {
            Write-Warning 'Could not request isolated Host shutdown'
        }
        # Host acknowledges shutdown before the SQLite WAL and file handles
        # are necessarily released. Never race its persistence shutdown.
        for ($attempt = 0; $attempt -lt 24; $attempt++) {
            try {
                $status = Invoke-AleraJson -Arguments @(
                    'runtime', '--runtime-dir', $runtime, '--json', 'status'
                )
                if ($status.running -eq $false) {
                    break
                }
            } catch {
                # The control socket may disappear during normal shutdown.
            }
            Start-Sleep -Milliseconds 250
        }
    }
    if ($null -ne $launcher) {
        try {
            $launcher.Refresh()
            if (-not $launcher.HasExited) {
                # Only the CLI process this script started, never user's Alera.
                Stop-Process -Id $launcher.Id -Force -ErrorAction SilentlyContinue
            }
        } catch {
            # The launcher may already have exited.
        }
    }
    if (-not $KeepArtifacts) {
        # Delete only the unique directory whose nonexistence was checked above.
        for ($attempt = 0; $attempt -lt 12; $attempt++) {
            try {
                Remove-Item -LiteralPath $Root -Recurse -Force -ErrorAction Stop
                break
            } catch {
                if ($attempt -eq 11) {
                    throw "Isolated smoke fixture cleanup failed: $($_.Exception.Message)"
                }
                Start-Sleep -Milliseconds 400
            }
        }
        if (Test-Path -LiteralPath $Root) {
            throw 'Isolated smoke fixture still exists after cleanup'
        }
    }
}
