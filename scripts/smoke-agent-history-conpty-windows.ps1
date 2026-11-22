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
# Optional: -TestNativeContentProbe uses fresh EMPTY Claude/Codex account homes,
# attempts to display fake native transcript markers with installed CLIs, and
# explicitly reports NOT VERIFIED when authentication is required.
# Optional: -TestVerifiedHistoryRpc -TestOfflineHistoryCommand injects
# inert official-name CLI shims into the NEW isolated Host process only,
# verifying exact UUID/selected account/JSONL through its real ConPTY.
# Installed provider CLI negatives are skipped in this offline shim mode;
# use -TestVerifiedHistoryRpc without it for those native negative checks.
# The offline shim never proves that an installed model CLI restored a chat.
# It does NOT prove successful resume of a real session or GUI rendering.
#
# Optional -GuiExecutable adds an isolated Native GPUI window while the
# synthetic Host's verified Agent tabs are still running. GUI state, runtime,
# and CLI overrides apply ONLY to that new process. The test root must not
# inherit any existing Git worktree (GPUI discovers parent worktrees).
# -IsolatedNestedGitFixture creates a NEW standalone inner Git root and
# verifies it has exactly one worktree so restricted workspace roots are safe.
# -ValidateGuiGitIsolationOnly verifies this boundary and cleans up without
# launching a Host, provider shim, or GUI; only disposable fixture paths change.
# -GuiBootstrapOnly accepts a responsive native window after its bounded hold
# WITHOUT claiming user click-through. Without it, explicit release is required.
# Create gui-continue.flag in the unique -Root after visual acceptance; the
# harness owns and stops ONLY its own GUI/Host. The hold timeout also cleans up.
# This validates GUI bootstrap, not automated click-through or a native
# provider transcript restoration.

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$CliPath,
    [string]$Root = '',
    [switch]$TestVerifiedHistoryRpc,
    [switch]$TestNativeContentProbe,
    [switch]$TestOfflineHistoryCommand,
    [string]$GuiExecutable = '',
    [int]$GuiHoldSeconds = 150,
    [switch]$IsolatedNestedGitFixture,
    [switch]$ValidateGuiGitIsolationOnly,
    [switch]$GuiBootstrapOnly,
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
$guiLauncher = $null
$hostStarted = $false
$shimDir = $null
$guiReady = Join-Path $Root 'gui-ready.txt'
$guiContinue = Join-Path $Root 'gui-continue.flag'
if (-not [string]::IsNullOrWhiteSpace($GuiExecutable)) {
    if (-not $TestVerifiedHistoryRpc -or -not $TestOfflineHistoryCommand) {
        throw 'GUI fixture requires verified history RPC and inert offline commands'
    }
    if ($GuiHoldSeconds -lt 10 -or $GuiHoldSeconds -gt 180) {
        throw 'GUI fixture hold must be between 10 and 180 seconds'
    }
    $GuiExecutable = (Resolve-Path -LiteralPath $GuiExecutable -ErrorAction Stop).Path
}
if ($IsolatedNestedGitFixture -and [string]::IsNullOrWhiteSpace($GuiExecutable)) {
    throw 'A nested Git fixture is supported only with an isolated GUI smoke test'
}
if ($ValidateGuiGitIsolationOnly -and -not $IsolatedNestedGitFixture) {
    throw 'The Git isolation-only preflight requires -IsolatedNestedGitFixture'
}
if ($GuiBootstrapOnly -and ([string]::IsNullOrWhiteSpace($GuiExecutable) -or $ValidateGuiGitIsolationOnly)) {
    throw 'GUI bootstrap-only mode requires a real isolated GUI run, not a Git-only preflight'
}
if ($TestOfflineHistoryCommand -and -not $TestVerifiedHistoryRpc) {
    throw 'Offline mock mode requires -TestVerifiedHistoryRpc'
}
if ($TestOfflineHistoryCommand -and $TestNativeContentProbe) {
    throw 'Mock mode and installed native CLI probe cannot run together'
}
if ($TestOfflineHistoryCommand) {
    . "$PSScriptRoot/mock-history-launch-commands.ps1"
}
function Start-IsolatedHistoryHost {
    $options = @{
        FilePath = $CliPath
        ArgumentList = @('runtime', '--runtime-dir', ('"' + $runtime + '"'), '--json', 'start')
        WindowStyle = 'Hidden'
        PassThru = $true
    }
    if ($TestOfflineHistoryCommand) {
        $options.Environment = @{ PATH = ($shimDir + [IO.Path]::PathSeparator + $env:PATH) }
    }
    Start-Process @options
}

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
            $stderrOutput = $stderrTask.GetAwaiter().GetResult()
            $exitCode = $process.ExitCode
        } finally {
            $process.Dispose()
        }
        if ($exitCode -ne 0) {
            # This fixture owns only its fresh Runtime. Redact its path and
            # print a bounded error category instead of hiding an allocator
            # race behind the generic process exit code.
            $details = (([string]$stderrOutput) + ' ' + $raw)
            $diagnostic = if ($details -match '(?i)PTY worker startup failed:[\s\S]*?os error (\d+)') {
                "PTY Worker start: Windows OS error $($Matches[1])"
            } elseif ($details -match '(?i)(?:Host unavailable|connection refused|connection reset)') {
                'Host transport unavailable'
            } elseif ($details -match '(?i)(?:workspace.*not found|workspace.*missing)') {
                'Test workspace unavailable'
            } else {
                'Unclassified isolated Runtime failure (inspect Host logs)'
            }
            throw "Alera CLI '$label' exited with code $exitCode (fixture diagnostic: $diagnostic)"
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
        if ($TestOfflineHistoryCommand) {
            # Verify the exact Host-created Tab really owns a ConPTY with
            # source-account-scoped command arguments. No real provider runs.
            $live = Invoke-AleraJson -Arguments @(
                'terminal', '--runtime-dir', $runtime, '--no-start', '--json',
                'list', '--workspace', $workspaceId
            )
            $matches = @($live.items | Where-Object {
                $_.tabId -eq $resumed.tab.id -and $_.workspaceId -eq $workspaceId
            })
            if ($matches.Count -ne 1) {
                throw "$($entry.provider) Host did not create exactly one live historical terminal"
            }
            $sentinel = if ($entry.provider -eq 'codex') {
                'ALERA_ISOLATED_CODEX_RESUME_COMMAND_VERIFIED'
            } else {
                'ALERA_ISOLATED_CLAUDE_RESUME_COMMAND_VERIFIED'
            }
            if (-not (Wait-TerminalMatch ([string]$matches[0].handle) ([regex]::Escape($sentinel)) 15)) {
                throw "$($entry.provider) command did not pass exact UUID/account through ConPTY"
            }
            Write-Output ("$($entry.provider) Host -> ConPTY -> offline command/UUID/account: PASS")
        }

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
    if (-not [string]::IsNullOrWhiteSpace($GuiExecutable)) {
        # GUI obtains ONLY the fixture Runtime and isolated preview settings.
        # Both provider CLI scripts remain synthetic and unauthenticated.
        $guiState = Join-Path $Root 'gui-state'
        $null = New-Item -ItemType Directory -Path $guiState -Force
        $script:guiLauncher = Start-Process -FilePath $GuiExecutable -ArgumentList @('"' + $project + '"') -WorkingDirectory $project -Environment @{ ALERA_GPUI_STATE_DIR = $guiState; ALERA_RUNTIME_DIR = $runtime; ALERA_CLI_PATH = $CliPath } -RedirectStandardError (Join-Path $Root 'gui-stderr.log') -PassThru
        if ($null -eq $script:guiLauncher) {
            throw 'GUI preview did not return a process identity'
        }
        # No Host tokens or provider credentials are written in this marker.
        [IO.File]::WriteAllText($guiReady, [string]$script:guiLauncher.Id, [Text.UTF8Encoding]::new($false))
        Write-Output 'Isolated GPUI GUI: LAUNCHED; awaiting bounded bootstrap/explicit test release'
        $deadline = [DateTime]::UtcNow.AddSeconds($GuiHoldSeconds)
        while ([DateTime]::UtcNow -lt $deadline) {
            if (Test-Path -LiteralPath $guiContinue) {
                Write-Output 'Isolated GPUI GUI: explicit test release received'
                break
            }
            $script:guiLauncher.Refresh()
            if ($script:guiLauncher.HasExited) {
                throw 'Isolated GPUI GUI terminated before acceptance finished'
            }
            Start-Sleep -Milliseconds 400
        }
        if (-not (Test-Path -LiteralPath $guiContinue)) {
            if (-not $GuiBootstrapOnly) {
                throw 'Isolated GPUI GUI acceptance timed out'
            }
            # A bootstrap-only pass never claims history-card clicks or native
            # transcript recovery. Verify a live responsive owned HWND, not
            # merely that Start-Process returned a PID.
            $guiWindow = Get-Process -Id $script:guiLauncher.Id -ErrorAction Stop
            if ($guiWindow.MainWindowHandle -eq [IntPtr]::Zero -or -not $guiWindow.Responding) {
                throw 'Isolated GPUI launched but no responsive native window was verified'
            }
            Write-Output 'Isolated GPUI GUI: responsive native window PASS (bootstrap only; no click-through claim)'
        }
        try {
            $script:guiLauncher.Refresh()
            if (-not $script:guiLauncher.HasExited) {
                Stop-Process -Id $script:guiLauncher.Id -Force -ErrorAction Stop
            }
        } finally {
            $script:guiLauncher = $null
        }
        # Release fake CLI hold loops before the Host's orderly restart.
        [IO.File]::WriteAllText(
            (Join-Path $Root 'stop-gui-shims.flag'), 'done',
            [Text.UTF8Encoding]::new($false)
        )
        Start-Sleep -Milliseconds 600
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
    $script:launcher = Start-IsolatedHistoryHost
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
    if (-not [string]::IsNullOrWhiteSpace($GuiExecutable)) {
        # Reject inherited parent repositories. The opt-in path initializes a
        # new independent repository in the NEW empty test project directory.
        # Do not launch a GUI unless its sole Git root is the disposable one.
        $gitCommand = Get-Command git -ErrorAction Stop
        if ($IsolatedNestedGitFixture) {
            if (@(Get-ChildItem -LiteralPath $project -Force).Count -ne 0) {
                throw 'Nested Git isolation requires a newly created empty test project'
            }
            $null = & $gitCommand.Source -C $project init --quiet --initial-branch=main
            if ($LASTEXITCODE -ne 0) {
                throw 'Could not create an isolated, single-worktree Git project'
            }
        }
        $gitTop = & $gitCommand.Source -C $project rev-parse --show-toplevel 2>$null
        $hasGitRoot = $LASTEXITCODE -eq 0
        if ($IsolatedNestedGitFixture) {
            if (-not $hasGitRoot) {
                throw 'Isolated nested Git project has no verified repository root'
            }
            $expected = [IO.Path]::GetFullPath($project).TrimEnd('\', '/')
            $actual = [IO.Path]::GetFullPath([string]$gitTop).TrimEnd('\', '/')
            if (-not [string]::Equals($actual, $expected, [StringComparison]::OrdinalIgnoreCase)) {
                throw 'Nested Git discovery escaped the disposable test project'
            }
            $worktreeListing = @(& $gitCommand.Source -C $project worktree list --porcelain)
            if ($LASTEXITCODE -ne 0) {
                throw 'Cannot verify isolated nested Git worktree inventory'
            }
            $roots = @($worktreeListing | Where-Object { $_ -like 'worktree *' })
            if ($roots.Count -ne 1) {
                throw 'Isolated GUI fixture has unexpected additional Git worktrees'
            }
            $rootFromList = [IO.Path]::GetFullPath($roots[0].Substring(9)).TrimEnd('\', '/')
            if (-not [string]::Equals($rootFromList, $expected, [StringComparison]::OrdinalIgnoreCase)) {
                throw 'Isolated GUI fixture worktree does not match disposable project'
            }
            Write-Output 'Isolated nested Git root and single-worktree boundary: PASS'
        } elseif ($hasGitRoot) {
            throw 'Isolated GUI fixture root must be outside every existing Git Worktree'
        }
    }
    if ($ValidateGuiGitIsolationOnly) {
        Write-Output 'GUI Git fixture isolation-only preflight: PASS (no Host or GUI started)'
        return
    }

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

    if ($TestOfflineHistoryCommand) {
        $shimDir = New-IsolatedHistoryCommandShims -FixtureRoot $Root -KeepProcessAliveForGui:(-not [string]::IsNullOrWhiteSpace($GuiExecutable))
    }
    # The starter may wait for its detached child on some Windows shells.
    # Launch it independently and poll the authenticated, isolated Runtime.
    $launcher = Start-IsolatedHistoryHost
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
    # Status can turn green just before the ConPTY allocator is ready.
    Start-Sleep -Milliseconds 750
    Write-Output 'Isolated Host: PASS'
    if ($TestOfflineHistoryCommand) {
        $probe = New-IsolatedTerminal
        Start-Sleep -Milliseconds 650
        Send-TerminalCommand $probe 'where.exe codex'
        if (-not (Wait-TerminalMatch $probe 'isolated-cli-shims[\\/]codex\.cmd' 10)) {
            $proofText = Read-TerminalText $probe
            $inPath = $proofText -match 'isolated-cli-shims'
            $hasCodex = $proofText -match 'codex(?:\.ps1|\.exe|\.cmd)'
            Write-Output ("Isolated command diagnosis: shim path visible=$inPath; codex command visible=$hasCodex")
            throw 'Test Host shell did not resolve isolated codex.cmd before the installed provider'
        }
        Write-Output 'Isolated Host official-name command routing: PASS'
    }

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

    if (-not $TestOfflineHistoryCommand) {
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
    } else {
        Write-Output 'Installed CLI negative probes: SKIPPED in offline shim mode'
    }
    if ($TestNativeContentProbe) {
        . "$PSScriptRoot/native-session-content-probe.ps1"
        Invoke-IsolatedNativeContentProbe -FixtureRoot $Root -WorkspacePath $project
    }
    if ($TestVerifiedHistoryRpc) {
        Invoke-VerifiedHistoryRpcFixture
        if ($TestOfflineHistoryCommand) {
            Write-Output 'Result: PASS (offline Host->ConPTY exact argv and persisted Tab; genuine native restore UNVERIFIED)'
        } else {
            Write-Output 'Result: PASS (native negative + synthetic Host Resume; real-session restore unverified)'
        }
    } else {
        Write-Output 'Result: PASS (negative native Resume only; real-session restore unverified)'
    }
}
finally {
    if (-not [string]::IsNullOrWhiteSpace($GuiExecutable) -and
        (Test-Path -LiteralPath $Root)) {
        try {
            # The fixture's own command shims cannot remain alive indefinitely
            # if visual acceptance or Host startup exits with an exception.
            [IO.File]::WriteAllText(
                (Join-Path $Root 'stop-gui-shims.flag'), 'done',
                [Text.UTF8Encoding]::new($false)
            )
        } catch { }
    }
    if ($null -ne $guiLauncher) {
        try {
            $guiLauncher.Refresh()
            if (-not $guiLauncher.HasExited) {
                # Cleanup only the exact preview process started here.
                Stop-Process -Id $guiLauncher.Id -Force -ErrorAction SilentlyContinue
            }
        } catch { }
    }
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
