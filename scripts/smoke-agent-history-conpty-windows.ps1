# Windows-only, negative native-UUID smoke test for Alera's real ConPTY Host.
# Uses a fresh Runtime + empty workspace. It NEVER resumes an existing
# conversation, writes a model prompt, or mutates a provider auth directory.
#
# Example:
#   pwsh -NoProfile -File scripts/smoke-agent-history-conpty-windows.ps1 `
#     -CliPath D:\build\alera.exe
# Optional deeper isolated Host search→Profile→Tab test:
#   pwsh -NoProfile -File scripts/smoke-agent-history-conpty-windows.ps1 `
#     -CliPath D:\build\alera.exe -TestVerifiedHistoryRpc
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
    [switch]$TestVerifiedHistoryRpc,
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

# Authenticated loopback RPC to the unique test Runtime. Never use a normal
# Alera control file; no token or provider content is printed or persisted.
function Invoke-IsolatedHostRpc {
    param([string]$Method, [hashtable]$Payload)
    $control = Get-Content -LiteralPath (Join-Path $runtime 'runtime-host.json') -Raw |
        ConvertFrom-Json
    $connection = [System.Net.Sockets.TcpClient]::new()
    $connection.Connect('127.0.0.1', [int]$control.port)
    $stream = $connection.GetStream()
    $stream.ReadTimeout = 47000
    $stream.WriteTimeout = 47000
    $utf8 = [System.Text.UTF8Encoding]::new($false)
    $reader = [System.IO.StreamReader]::new($stream, $utf8, $false, 4096, $true)
    $writer = [System.IO.StreamWriter]::new($stream, $utf8, 4096, $true)
    $writer.AutoFlush = $true
    try {
        $requests = @(
            @{ id = 1; type = 'hello'; payload = @{
                protocolVersion = [int]$control.protocolVersion
                token = [string]$control.token
                clientKind = 'cli'
            } },
            @{ id = 2; type = $Method; payload = $Payload }
        )
        foreach ($request in $requests) {
            $writer.WriteLine(($request | ConvertTo-Json -Depth 64 -Compress))
            $deadline = [DateTime]::UtcNow.AddSeconds(45)
            $reply = $null
            while ([DateTime]::UtcNow -lt $deadline) {
                $line = $reader.ReadLine()
                if ($null -eq $line) {
                    throw "Test Host closed the RPC socket during $($request.type)"
                }
                $frame = $line | ConvertFrom-Json
                # Host may publish asynchronous events without a request id.
                if (($frame.PSObject.Properties.Name -contains 'id') -and
                    $null -ne $frame.id -and
                    [int]$frame.id -eq [int]$request.id) {
                    $reply = $frame
                    break
                }
            }
            if ($null -eq $reply) {
                throw "Test Host did not acknowledge $($request.type)"
            }
            if ($reply.ok -ne $true) {
                $reason = ([string]$reply.error)
                if ($reason.Length -gt 200) {
                    $reason = $reason.Substring(0, 200)
                }
                throw "Test Host rejected $($request.type): $reason"
            }
        }
        return $reply.payload
    }
    finally {
        $writer.Dispose()
        $reader.Dispose()
        $stream.Dispose()
        $connection.Dispose()
    }
}

function Invoke-VerifiedHistoryRpcFixture {
    # These account directories contain only synthetic native metadata and
    # NO copied provider credentials, user history or token files.
    $codexHome = Join-Path $Root 'codex-test-account'
    $claudeHome = Join-Path $Root 'claude-test-account'
    $codexSessions = Join-Path $codexHome 'sessions/2026/11/21'
    $claudeSessions = Join-Path $claudeHome 'projects/-isolated'
    $null = New-Item -ItemType Directory -Path $codexSessions -Force
    $null = New-Item -ItemType Directory -Path $claudeSessions -Force
    $encoding = [System.Text.UTF8Encoding]::new($false)
    $codexId = [guid]::NewGuid().ToString('D')
    $claudeId = [guid]::NewGuid().ToString('D')
    $codexFile = Join-Path $codexSessions ("rollout-2026-11-21T00-00-00-$codexId.jsonl")
    $claudeFile = Join-Path $claudeSessions ("$claudeId.jsonl")
    $codexMeta = @{ type = 'session_meta'; payload = @{
        id = $codexId; cwd = $project
    } } | ConvertTo-Json -Compress -Depth 10
    $codexMessage = @{ type = 'event_msg'; payload = @{
        type = 'user_message'; message = 'alera protected history fixture codex'
    } } | ConvertTo-Json -Compress -Depth 10
    [IO.File]::WriteAllText($codexFile, "$codexMeta`n$codexMessage`n", $encoding)
    $claudeMeta = @{ type = 'user'; sessionId = $claudeId; cwd = $project;
        message = @{ role = 'user'; content = 'alera protected history fixture claude' }
    } | ConvertTo-Json -Compress -Depth 10
    [IO.File]::WriteAllText($claudeFile, "$claudeMeta`n", $encoding)

    $settings = Invoke-IsolatedHostRpc -Method 'runtimeSettings.get' -Payload @{}
    $quotas = $settings.agentQuotas
    $quotas.codexProfiles = @(@{ alias = 'Isolated Codex'; profile = $codexHome })
    $quotas.claudeProfiles = @(@{
        alias = 'Isolated Claude'; profile = $claudeHome
        showInUsage = $true; usageDisplayName = $null
    })
    $saved = Invoke-IsolatedHostRpc -Method 'runtimeSettings.update' -Payload @{
        expectedRevision = [int]$settings.revision
        agentQuotas = $quotas
    }
    if (@($saved.agentQuotas.codexProfiles).Count -ne 1 -or
        @($saved.agentQuotas.claudeProfiles).Count -ne 1) {
        throw 'Test Host did not persist isolated provider homes'
    }

    $savedTabs = @()
    foreach ($entry in @(
        @{ provider = 'codex'; home = $codexHome; id = $codexId },
        @{ provider = 'claude'; home = $claudeHome; id = $claudeId }
    )) {
        $profile = Invoke-AleraJson -Arguments @(
            'agent-profile', '--runtime-dir', $runtime, '--no-start', '--json',
            'create', '--name', ('isolated-' + $entry.provider),
            '--agent-type', $entry.provider, '--launch-mode', 'command',
            '--command', $entry.provider
        )
        if ([string]::IsNullOrWhiteSpace([string]$profile.id)) {
            throw "Missing $($entry.provider) Agent Profile id"
        }

        # Prove the exact account and original registered Worktree were found.
        $search = Invoke-IsolatedHostRpc -Method 'agentHistory.search' -Payload @{
            text = 'alera protected history fixture'
            consentToIndexUserText = $true
            provider = $entry.provider
            accountId = $entry.home
            projectId = $projectId
            workspaceId = $workspaceId
            maxResults = 5
        }
        $found = @($search.hits | Where-Object {
            $_.session.providerSessionId -eq $entry.id -and
            $_.session.providerAccountId -eq $entry.home
        })
        if ($found.Count -ne 1) {
            throw "History search did not find exactly one isolated $($entry.provider) source"
        }
        Write-Output ("$($entry.provider) Host history search: PASS")

        $locateArguments = @(
            'terminal', '--runtime-dir', $runtime, '--no-start', '--json',
            'history-locate', '--provider', $entry.provider,
            '--account-id', $entry.home,
            '--project-id', $projectId, '--workspace-id', $workspaceId,
            '--session-id', $entry.id
        )
        $before = Invoke-AleraJson -Arguments $locateArguments
        if ($before.found -ne $false -or $before.noAgentStarted -ne $true) {
            throw "$($entry.provider) read-only lookup invented a Tab before Resume"
        }
        Write-Output ("$($entry.provider) read-only initial lookup empty: PASS")

        $mutation = [guid]::NewGuid().ToString('D')
        $resumed = Invoke-AleraJson -Arguments @(
            'terminal', '--runtime-dir', $runtime, '--no-start', '--json',
            'history-resume', '--provider', $entry.provider,
            '--account-id', $entry.home,
            '--project-id', $projectId, '--workspace-id', $workspaceId,
            '--session-id', $entry.id, '--profile-id', [string]$profile.id,
            '--client-mutation-id', $mutation,
            '--confirm-account', '--confirm-resume'
        )
        if ([string]::IsNullOrWhiteSpace([string]$resumed.tab.id)) {
            throw "$($entry.provider) Host did not create a verified Resume Tab"
        }
        if ($resumed.tab.payload.PSObject.Properties.Name -contains 'agentHistoryResumeOriginV1') {
            throw "$($entry.provider) exposed private account origin in Tab response"
        }
        foreach ($promptField in @('initialPrompt', 'pendingAgentPrompt')) {
            if (($resumed.tab.payload.PSObject.Properties.Name -contains $promptField) -and
                $null -ne $resumed.tab.payload.$promptField) {
                throw "$($entry.provider) Resume Tab attempted to replay $promptField"
            }
        }
        Write-Output ("$($entry.provider) verified source -> Profile -> Tab: PASS")

        # A *separate* no-start CLI process and local RPC connection must find
        # the exact saved Tab without knowing the old mutation UUID. This is
        # the recovery path after Classic/GPUI restarts with no pending state.
        $located = Invoke-AleraJson -Arguments $locateArguments
        if ($located.found -ne $true -or
            $located.tabId -ne $resumed.tab.id -or
            $located.workspaceId -ne $workspaceId -or
            $located.noAgentStarted -ne $true) {
            throw "$($entry.provider) read-only lookup did not recover original Tab"
        }
        Write-Output ("$($entry.provider) fresh-client exact Tab lookup: PASS")
        $savedTabs += @{
            provider = $entry.provider
            home = $entry.home
            id = $entry.id
            tabId = $resumed.tab.id
        }

        # Ambiguous ACK retries must return the *same* receipt and never
        # create a second PTY. A new mutation ID for the same UUID must fail.
        $replayed = Invoke-AleraJson -Arguments @(
            'terminal', '--runtime-dir', $runtime, '--no-start', '--json',
            'history-resume', '--provider', $entry.provider,
            '--account-id', $entry.home,
            '--project-id', $projectId, '--workspace-id', $workspaceId,
            '--session-id', $entry.id, '--profile-id', [string]$profile.id,
            '--client-mutation-id', $mutation,
            '--confirm-account', '--confirm-resume'
        )
        if ($replayed.tab.id -ne $resumed.tab.id) {
            throw "$($entry.provider) ambiguous resume retry did not replay same Tab"
        }
        Write-Output ("$($entry.provider) idempotent Resume retry: PASS")
        $conflict = $false
        try {
            $null = Invoke-IsolatedHostRpc -Method 'agentHistory.resumeExact' -Payload @{
                provider = $entry.provider
                accountId = $entry.home
                projectId = $projectId
                workspaceId = $workspaceId
                providerSessionId = $entry.id
                profileId = [string]$profile.id
                clientMutationId = [guid]::NewGuid().ToString('D')
                confirmProviderAccount = $true
                confirmResume = $true
            }
        } catch {
            $conflict = $_.Exception.Message -match 'already has an Alera Tab'
        }
        if (-not $conflict) {
            throw "$($entry.provider) fresh mutation duplicate owner was not rejected"
        }
        Write-Output ("$($entry.provider) duplicate Resume owner rejected: PASS")
    }
    # Stop and restart *only* the unique test Host. The new process has no
    # knowledge of any prior CLI mutation UUID, yet must locate the saved
    # same-source Tab after RuntimeStore is reopened from disk.
    $null = & $CliPath runtime --runtime-dir $runtime --json stop --force 2>&1
    $stopped = $false
    for ($attempt = 0; $attempt -lt 50; $attempt++) {
        try {
            $state = Invoke-AleraJson -Arguments @(
                'runtime', '--runtime-dir', $runtime, '--json', 'status'
            )
            if ($state.running -eq $false) {
                $stopped = $true
                break
            }
        } catch {
            # This unique Host's control file may disappear during shutdown.
        }
        Start-Sleep -Milliseconds 250
    }
    if (-not $stopped) {
        throw 'Isolated Host did not stop for restart lookup'
    }
    $script:hostStarted = $false
    if ($null -ne $script:launcher) {
        try {
            $script:launcher.Refresh()
            if (-not $script:launcher.HasExited) {
                # Stop only the detached start CLI process this script owns.
                Stop-Process -Id $script:launcher.Id -Force -ErrorAction SilentlyContinue
            }
        } catch {
            # It may have exited as soon as the isolated Host shut down.
        }
    }
    $script:launcher = Start-Process -FilePath $CliPath -ArgumentList @(
        'runtime', '--runtime-dir', ('"' + $runtime + '"'), '--json', 'start'
    ) -WindowStyle Hidden -PassThru
    $ready = $false
    for ($attempt = 0; $attempt -lt 90; $attempt++) {
        try {
            $state = Invoke-AleraJson -Arguments @(
                'runtime', '--runtime-dir', $runtime, '--json', 'status'
            )
            if ($state.running -eq $true) {
                $ready = $true
                $script:hostStarted = $true
                break
            }
        } catch {
            # New isolated Host may not yet have published its control file.
        }
        Start-Sleep -Milliseconds 500
    }
    if (-not $ready) {
        throw 'Isolated Host did not return after restart'
    }
    foreach ($saved in $savedTabs) {
        $located = Invoke-AleraJson -Arguments @(
            'terminal', '--runtime-dir', $runtime, '--no-start', '--json',
            'history-locate', '--provider', $saved.provider,
            '--account-id', $saved.home,
            '--project-id', $projectId, '--workspace-id', $workspaceId,
            '--session-id', $saved.id
        )
        if ($located.found -ne $true -or $located.tabId -ne $saved.tabId -or
            $located.workspaceId -ne $workspaceId -or
            $located.noAgentStarted -ne $true) {
            throw "$($saved.provider) Host-restart lookup lost saved exact Tab"
        }
        Write-Output ("$($saved.provider) Host-restart read-only Tab lookup: PASS")
    }
    Write-Output 'Native positive restoration: NOT TESTED (synthetic metadata is not a native conversation)'
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
    if ($TestVerifiedHistoryRpc) {
        Invoke-VerifiedHistoryRpcFixture
        Write-Output 'Result: PASS (native negative + synthetic Host Resume; real-session restore unverified)'
    } else {
        Write-Output 'Result: PASS (negative native Resume only; real-session restore unverified)'
    }
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
