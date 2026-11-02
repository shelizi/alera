param(
    [Parameter(Mandatory = $true)]
    [string]$AleraExe
)

# Windows integration smoke: private temp Runtime Host, Project, Workspace and
# PTY. No changes to the normal Alera profile or other worktrees.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if (-not (Test-Path -LiteralPath $AleraExe -PathType Leaf)) {
    throw "Alera executable not found: $AleraExe"
}

$root = Join-Path ([System.IO.Path]::GetTempPath()) ('alera-mcp-integration-' + [guid]::NewGuid().ToString('N'))
$runtime = Join-Path $root 'runtime'
$workspaceRoot = Join-Path $root 'sample-project'
$projectId = 'mcp-project-' + [guid]::NewGuid().ToString('N')
$workspaceId = 'mcp-workspace-' + [guid]::NewGuid().ToString('N')
$savedHandle = [Environment]::GetEnvironmentVariable('ALERA_TERMINAL_HANDLE', 'Process')
$savedWorkspace = [Environment]::GetEnvironmentVariable('ALERA_WORKSPACE_ID', 'Process')
$hostStarted = $false

function Invoke-AleraJson {
    param([string[]]$Arguments)
    $output = & $AleraExe @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw ('Alera CLI failed: ' + ($Arguments -join ' '))
    }
    return (($output -join "`n") | ConvertFrom-Json)
}

function Invoke-Mcp {
    param([object[]]$Requests)
    $lines = @(
        foreach ($request in $Requests) {
            ConvertTo-Json -InputObject $request -Compress -Depth 32
        }
    )
    $output = @($lines | & $AleraExe mcp --runtime-dir $runtime serve)
    if ($LASTEXITCODE -ne 0) {
        throw ('alera mcp serve failed, exit=' + $LASTEXITCODE)
    }
    $responses = @(
        foreach ($line in $output) {
            $line | ConvertFrom-Json
        }
    )
    return ,$responses
}

function Invoke-TestHostRpc {
    param([string]$Method, [object]$Payload)
    # This intentionally uses the private socket ONLY in the disposable test
    # profile, for AgentPresence fixture injection (no equivalent CLI verb).
    # Never print or persist the temporary Host credential.
    $control = (Get-Content -LiteralPath (Join-Path $runtime 'runtime-host.json') -Raw) | ConvertFrom-Json
    $tcp = New-Object System.Net.Sockets.TcpClient
    try {
        $tcp.Connect('127.0.0.1', [int]$control.port)
        $tcp.ReceiveTimeout = 5000
        $tcp.SendTimeout = 5000
        $stream = $tcp.GetStream()
        $writer = New-Object System.IO.StreamWriter($stream, (New-Object System.Text.UTF8Encoding($false)))
        $reader = New-Object System.IO.StreamReader($stream, (New-Object System.Text.UTF8Encoding($false)))
        $writer.AutoFlush = $true
        $writer.WriteLine((ConvertTo-Json -InputObject @{
            id = 1; type = 'hello'; payload = @{
                protocolVersion = $control.protocolVersion
                token = $control.token
                clientKind = 'cli'
            }
        } -Compress -Depth 25))
        $auth = $null
        for ($i = 0; $i -lt 20; $i++) {
            $line = $reader.ReadLine()
            if ($null -eq $line) { break }
            $frame = $line | ConvertFrom-Json
            if (($frame.PSObject.Properties.Name -contains 'id') -and $frame.id -eq 1) {
                $auth = $frame
                break
            }
        }
        if ($null -eq $auth -or $auth.ok -ne $true) {
            throw 'Fixture Host socket authentication failed'
        }
        $writer.WriteLine((ConvertTo-Json -InputObject @{
            id = 2; type = $Method; payload = $Payload
        } -Compress -Depth 25))
        for ($i = 0; $i -lt 20; $i++) {
            $line = $reader.ReadLine()
            if ($null -eq $line) { break }
            $frame = $line | ConvertFrom-Json
            if (($frame.PSObject.Properties.Name -contains 'id') -and $frame.id -eq 2) {
                if ($frame.ok -ne $true) {
                    throw "Fixture Host request failed: $Method"
                }
                return $frame.payload
            }
        }
        throw "No Host response for test fixture request: $Method"
    }
    finally {
        $tcp.Dispose()
    }
}

function Assert-McpResult {
    param([object[]]$Responses, [int]$Id, [bool]$IsError)
    $hit = @($Responses | Where-Object { $_.id -eq $Id })
    if ($hit.Count -ne 1 -or $null -eq $hit[0].result) {
        throw "MCP response missing for id=$Id"
    }
    if ($hit[0].result.isError -ne $IsError) {
        throw "MCP id=$Id returned isError=$($hit[0].result.isError), expected $IsError"
    }
    return $hit[0].result
}

New-Item -ItemType Directory -Path $workspaceRoot -Force | Out-Null
New-Item -ItemType Directory -Path (Join-Path $workspaceRoot 'src') -Force | Out-Null
[System.IO.File]::WriteAllText((Join-Path $workspaceRoot 'src\mcp_test_file.rs'), 'read-only fixture')

try {
    # Avoid capturing the starter's stdout through a PowerShell pipeline:
    # Windows background Host descendants can otherwise keep the pipe open
    # after the short-lived CLI has exited, hanging the smoke harness.
    $starter = Start-Process -FilePath $AleraExe -ArgumentList @(
        'runtime', '--runtime-dir', $runtime, '--json', 'start'
    ) -WindowStyle Hidden -PassThru
    if (-not $starter.WaitForExit(20000)) {
        $starter.Kill()
        throw 'Isolated Runtime Host starter did not return within 20 seconds'
    }
    if ($starter.ExitCode -ne 0) {
        throw "Isolated Runtime Host starter exited $($starter.ExitCode)"
    }
    $hostStarted = $true
    $status = Invoke-AleraJson -Arguments @('runtime', '--runtime-dir', $runtime, '--json', 'status')
    if ($status.running -ne $true) {
        throw 'Isolated Runtime Host failed to become ready'
    }
    $project = Invoke-AleraJson -Arguments @(
        'project', '--runtime-dir', $runtime, '--json', 'add',
        '--id', $projectId, '--name', 'MCP smoke project',
        '--repo-path', $workspaceRoot, '--kind', 'folder'
    )
    if ($project.id -ne $projectId) {
        throw 'Isolated Project was not created'
    }
    $workspace = Invoke-AleraJson -Arguments @(
        'workspace', '--runtime-dir', $runtime, '--json', 'register',
        '--id', $workspaceId, '--project-id', $projectId, '--name', 'MCP smoke workspace',
        '--path', $workspaceRoot, '--kind', 'main'
    )
    if ($workspace.id -ne $workspaceId) {
        throw 'Isolated Workspace was not registered'
    }
    $tab = Invoke-AleraJson -Arguments @(
        'tab', '--runtime-dir', $runtime, '--json', 'create',
        '--workspace-id', $workspaceId, '--title', 'MCP smoke terminal', '--spawn'
    )
    if ($tab.kind -ne 'terminal') {
        throw 'Isolated terminal tab was not created'
    }
    $terminal = $null
    for ($i = 0; $i -lt 30; $i++) {
        $inventory = Invoke-AleraJson -Arguments @(
            'terminal', '--runtime-dir', $runtime, '--json', 'list', '--workspace', $workspaceId
        )
        $alive = @($inventory.items | Where-Object { $_.running -eq $true })
        if ($alive.Count -gt 0) {
            $terminal = $alive[0]
            break
        }
        Start-Sleep -Milliseconds 200
    }
    if ($null -eq $terminal) {
        throw 'No running Host-managed PTY Session was created'
    }

    $env:ALERA_TERMINAL_HANDLE = $terminal.handle
    $env:ALERA_WORKSPACE_ID = $workspaceId
    $null = Invoke-TestHostRpc -Method 'orchestration.agentStatus' -Payload @{
        entries = @(@{
            terminalSessionId = $terminal.handle
            state = 'working'
            agentType = 'codex'
            prompt = 'MCP isolated test task'
            toolName = 'ReadFile'
            toolInput = 'DO_NOT_EXPOSE_RAW_TOOL_INPUT_SENTINEL'
            lastAssistantMessage = 'MCP isolated agent analysis'
        })
    }
    $null = Invoke-AleraJson -Arguments @(
        'terminal', '--runtime-dir', $runtime, '--json', 'write',
        '--handle', $terminal.handle, '--text', 'echo MCP_CURSOR_TEST_MARKER', '--enter'
    )

    $requests = @(
        @{jsonrpc='2.0'; id=1; method='initialize'; params=@{protocolVersion='2025-06-18'; capabilities=@{}; clientInfo=@{name='alera-mcp-smoke'; version='1'}}},
        @{jsonrpc='2.0'; method='notifications/initialized'},
        @{jsonrpc='2.0'; id=2; method='tools/list'},
        @{jsonrpc='2.0'; id=3; method='tools/call'; params=@{name='alera.list_projects'; arguments=@{query='MCP smoke'; limit=10}}},
        @{jsonrpc='2.0'; id=4; method='tools/call'; params=@{name='alera.list_workspaces'; arguments=@{projectId=$projectId}}},
        @{jsonrpc='2.0'; id=5; method='tools/call'; params=@{name='alera.list_tabs'; arguments=@{workspaceId=$workspaceId}}},
        @{jsonrpc='2.0'; id=6; method='tools/call'; params=@{name='alera.search_files'; arguments=@{query='mcp_test_file'; limit=10}}},
        @{jsonrpc='2.0'; id=7; method='tools/call'; params=@{name='alera.search_agents'; arguments=@{query='codex'}}},
        @{jsonrpc='2.0'; id=8; method='tools/call'; params=@{name='alera.list_tabs'; arguments=@{workspaceId='foreign-workspace'}}},
        @{jsonrpc='2.0'; id=9; method='tools/call'; params=@{name='alera.read_agent_output'; arguments=@{handle='foreign-agent'}}},
        @{jsonrpc='2.0'; id=11; method='tools/call'; params=@{name='alera.get_agent_activity'; arguments=@{handle=$terminal.handle}}},
        @{jsonrpc='2.0'; id=12; method='tools/call'; params=@{name='alera.read_agent_output'; arguments=@{handle=$terminal.handle; maxBytes=4096}}},
        @{jsonrpc='2.0'; id=14; method='tools/call'; params=@{name='alera.read_agent_output'; arguments=@{handle=$terminal.handle; maxBytes=8193}}},
        @{jsonrpc='2.0'; id=15; method='tools/call'; params=@{name='alera.list_projects'; arguments=@{limit=999}}},
        @{jsonrpc='2.0'; id=18; method='tools/call'; params=@{name='alera.open_file'; arguments=@{relativePath='src/mcp_test_file.rs'}}},
        @{jsonrpc='2.0'; id=19; method='tools/call'; params=@{name='alera.compare_branches'; arguments=@{baseRef='main'; headRef='dev'}}}
    )
    $responses = Invoke-Mcp -Requests $requests
    if ($responses.Count -ne ($requests.Count - 1)) {
        throw "Unexpected response count: $($responses.Count)"
    }
    $init = @($responses | Where-Object { $_.id -eq 1 })[0]
    if ($init.result.protocolVersion -ne '2025-06-18') {
        throw 'MCP initialization failed'
    }
    $listing = @($responses | Where-Object { $_.id -eq 2 })[0]
    if ($listing.result.tools.Count -ne 11) {
        throw "Expected 11 tools (7 read-only, 4 guarded UI), found $($listing.result.tools.Count)"
    }
    if (@($listing.result.tools | Where-Object { $_.annotations.readOnlyHint -eq $true }).Count -ne 7) {
        throw 'Expected seven read-only tools'
    }
    $projects = Assert-McpResult -Responses $responses -Id 3 -IsError $false
    if (@($projects.structuredContent.items | Where-Object { $_.id -eq $projectId }).Count -ne 1) {
        throw 'MCP cannot find its isolated Project'
    }
    $workspaces = Assert-McpResult -Responses $responses -Id 4 -IsError $false
    if (@($workspaces.structuredContent.items | Where-Object { $_.id -eq $workspaceId }).Count -ne 1) {
        throw 'MCP cannot find its isolated Workspace'
    }
    $tabs = Assert-McpResult -Responses $responses -Id 5 -IsError $false
    if (@($tabs.structuredContent.items | Where-Object { $_.id -eq $tab.id }).Count -ne 1) {
        throw 'MCP cannot find its isolated Tab'
    }
    $files = Assert-McpResult -Responses $responses -Id 6 -IsError $false
    if (@($files.structuredContent.items | Where-Object { $_.relativePath -eq 'src/mcp_test_file.rs' }).Count -ne 1) {
        throw 'MCP file search failed to find fixture'
    }
    $agents = Assert-McpResult -Responses $responses -Id 7 -IsError $false
    if ($agents.structuredContent.kind -ne 'agents' -or
        @($agents.structuredContent.items | Where-Object { $_.handle -eq $terminal.handle }).Count -ne 1) {
        throw 'MCP did not find its test Agent presence'
    }
    if (($agents.structuredContent | ConvertTo-Json -Compress -Depth 15) -match
        'DO_NOT_EXPOSE_RAW_TOOL_INPUT_SENTINEL') {
        throw 'MCP Agent search leaked a sensitive raw tool input'
    }
    $null = Assert-McpResult -Responses $responses -Id 8 -IsError $true
    $null = Assert-McpResult -Responses $responses -Id 9 -IsError $true
    $activity = Assert-McpResult -Responses $responses -Id 11 -IsError $false
    if ($activity.structuredContent.agent.agentState -ne 'working' -or
        $activity.structuredContent.agent.toolName -ne 'ReadFile') {
        throw 'MCP Agent activity differs from reported Host Agent presence'
    }
    if (($activity.structuredContent | ConvertTo-Json -Compress -Depth 15) -match
        'DO_NOT_EXPOSE_RAW_TOOL_INPUT_SENTINEL') {
        throw 'MCP Agent activity leaked a sensitive raw tool input'
    }
    $output = Assert-McpResult -Responses $responses -Id 12 -IsError $false
    if ($output.structuredContent.kind -ne 'agentOutput' -or
        $output.structuredContent.handle -ne $terminal.handle) {
        throw 'MCP Agent output read did not return the live session'
    }
    $null = Assert-McpResult -Responses $responses -Id 14 -IsError $true
    $null = Assert-McpResult -Responses $responses -Id 15 -IsError $true
    $null = Assert-McpResult -Responses $responses -Id 18 -IsError $true
    $null = Assert-McpResult -Responses $responses -Id 19 -IsError $true
    $hasCursorMarker = $false
    for ($i = 0; $i -lt 20; $i++) {
        $read = Invoke-Mcp -Requests @(
            @{jsonrpc='2.0'; id=16; method='tools/call'; params=@{name='alera.read_agent_output'; arguments=@{handle=$terminal.handle; cursor=0; maxBytes=8192}}}
        )
        $page = Assert-McpResult -Responses $read -Id 16 -IsError $false
        if ($page.structuredContent.text -match 'MCP_CURSOR_TEST_MARKER') {
            $hasCursorMarker = $true
            $nextCursor = [long]$page.structuredContent.nextCursor
            $next = Invoke-Mcp -Requests @(
                @{jsonrpc='2.0'; id=17; method='tools/call'; params=@{name='alera.read_agent_output'; arguments=@{handle=$terminal.handle; cursor=$nextCursor; maxBytes=8192}}}
            )
            $incremental = Assert-McpResult -Responses $next -Id 17 -IsError $false
            if ([long]$incremental.structuredContent.cursor -lt $nextCursor -or
                $incremental.structuredContent.truncated -ne $false) {
                throw 'Incremental Terminal output cursor returned overlapping or truncated data'
            }
            break
        }
        Start-Sleep -Milliseconds 200
    }
    if (-not $hasCursorMarker) {
        throw 'No known Terminal output marker was observed through MCP'
    }

    # Spoofing an unknown/stale Terminal handle must not expose Tab records.
    $env:ALERA_TERMINAL_HANDLE = 'not-a-real-terminal'
    $invalid = Invoke-Mcp -Requests @(
        @{jsonrpc='2.0'; id=10; method='tools/call'; params=@{name='alera.list_tabs'; arguments=@{}}}
    )
    $null = Assert-McpResult -Responses $invalid -Id 10 -IsError $true

    Write-Output 'PASS isolated Host online and authenticated'
    Write-Output 'PASS MCP stdio initialize + 7 read-only and 4 gated UI tool descriptions'
    Write-Output 'PASS UI tools fail closed when PTY capability was not inherited'
    Write-Output 'PASS Host-backed projects, workspaces, tabs, file search, live Agent inventory'
    Write-Output 'PASS Host-injected Agent activity + bounded terminal output'
    Write-Output 'PASS Terminal output marker and incremental cursor'
    Write-Output 'PASS sensitive raw Agent tool input not exposed'
    Write-Output 'PASS cross-workspace and unknown-agent access denied'
    Write-Output 'PASS over-limit pagination and output size rejected'
    Write-Output 'PASS stale/unknown caller terminal access denied'
}
finally {
    [Environment]::SetEnvironmentVariable('ALERA_TERMINAL_HANDLE', $savedHandle, 'Process')
    [Environment]::SetEnvironmentVariable('ALERA_WORKSPACE_ID', $savedWorkspace, 'Process')
    if ($hostStarted) {
        try {
            $null = Invoke-AleraJson -Arguments @('runtime', '--runtime-dir', $runtime, '--json', 'stop', '--force')
        }
        catch {
            Write-Warning 'Unable to stop isolated test Runtime Host; inspect test runtime before cleanup'
            throw
        }
    }
    if (Test-Path -LiteralPath $root) {
        # Host shutdown may answer before Windows releases SQLite/PTY handles.
        # A passing smoke test must not silently leave temporary runtimes.
        for ($attempt = 0; $attempt -lt 30; $attempt++) {
            try {
                Remove-Item -LiteralPath $root -Force -Recurse -ErrorAction Stop
                break
            }
            catch {
                Start-Sleep -Milliseconds 300
            }
        }
        if (Test-Path -LiteralPath $root) {
            throw 'Isolated MCP test Runtime could not be cleaned up'
        }
    }
}
