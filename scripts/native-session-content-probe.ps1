# Optional fixture used only from smoke-agent-history-conpty-windows.ps1.
# Generates fake native-format JSONL under the smoke test's fresh root.
# It has NO provider login, API key, prompts, or connection to a real user's
# histories. The real installed CLI must visibly render the *file* marker
# before positive restoration may be claimed.
function Invoke-IsolatedNativeContentProbe {
    param(
        [Parameter(Mandatory=$true)][string]$FixtureRoot,
        [Parameter(Mandatory=$true)][string]$WorkspacePath
    )
    if ($FixtureRoot -match '[&|<>^%!"\r\n]') {
        throw 'Unsafe fixture path for process-local cmd environment'
    }
    $enc = [Text.UTF8Encoding]::new($false)
    $stamp = '2026-11-22T00:00:00.000Z'
    $codexId = [guid]::NewGuid().ToString('D')
    $claudeId = [guid]::NewGuid().ToString('D')
    $codexHome = Join-Path $FixtureRoot 'empty-codex-account'
    $claudeHome = Join-Path $FixtureRoot 'empty-claude-account'
    $codexMarker = 'ALERA_NATIVE_CODEX_CONTENT_' + $codexId.Substring(0,8)
    $claudeMarker = 'ALERA_NATIVE_CLAUDE_CONTENT_' + $claudeId.Substring(0,8)
    $codexSessions = Join-Path $codexHome 'sessions/2026/11/22'
    New-Item -ItemType Directory -Path $codexSessions -Force | Out-Null
    $codexFile = Join-Path $codexSessions ("rollout-2026-11-22T00-00-00-$codexId.jsonl")
    $codexLines = @(
        (@{timestamp=$stamp;type='session_meta';payload=@{
            id=$codexId;timestamp=$stamp;cwd=$WorkspacePath;
            originator='codex_cli_rs';cli_version='0.169.0'
        }} | ConvertTo-Json -Compress -Depth 15),
        (@{timestamp=$stamp;type='event_msg';payload=@{
            type='user_message';message='fixture only, no prompt';images=@()
        }} | ConvertTo-Json -Compress -Depth 15),
        (@{timestamp=$stamp;type='response_item';payload=@{
            type='message';role='assistant';
            content=@(@{type='output_text';text=$codexMarker})
        }} | ConvertTo-Json -Compress -Depth 15)
    )
    [IO.File]::WriteAllText($codexFile,($codexLines -join "`n")+"`n",$enc)
    $slug = $WorkspacePath -replace '[^a-zA-Z0-9]', '-'
    $claudeSessions = Join-Path $claudeHome ('projects/' + $slug)
    New-Item -ItemType Directory -Path $claudeSessions -Force | Out-Null
    $claudeFile = Join-Path $claudeSessions ("$claudeId.jsonl")
    $parent = [guid]::NewGuid().ToString('D')
    $claudeLines = @(
        (@{type='user';sessionId=$claudeId;cwd=$WorkspacePath;
          timestamp=$stamp;uuid=$parent;parentUuid=$null;
          isSidechain=$false;userType='external';version='2.1.330';
          message=@{role='user';content='fixture only, no prompt'}
        } | ConvertTo-Json -Compress -Depth 15),
        (@{type='assistant';sessionId=$claudeId;cwd=$WorkspacePath;
          timestamp=$stamp;uuid=([guid]::NewGuid().ToString('D'));
          parentUuid=$parent;isSidechain=$false;userType='external';
          version='2.1.330';message=@{role='assistant';model='claude-sonnet-4-5';
            content=@(@{type='text';text=$claudeMarker});stop_reason='end_turn'}
        } | ConvertTo-Json -Compress -Depth 15)
    )
    [IO.File]::WriteAllText($claudeFile,($claudeLines -join "`n")+"`n",$enc)

    foreach ($entry in @(
        @{ name='Codex'; env='CODEX_HOME'; home=$codexHome; marker=$codexMarker;
          command=('codex --no-daemon -c check_for_update_on_startup=false resume '+$codexId);
          clear=@('OPENAI_API_KEY','CODEX_API_KEY','AZURE_OPENAI_API_KEY','OPENAI_BASE_URL') },
        @{ name='Claude'; env='CLAUDE_CONFIG_DIR'; home=$claudeHome; marker=$claudeMarker;
          command=('claude --resume '+$claudeId);
          clear=@('ANTHROPIC_API_KEY','CLAUDE_CODE_OAUTH_TOKEN',
            'CLAUDE_CODE_USE_BEDROCK','CLAUDE_CODE_USE_VERTEX',
            'AWS_ACCESS_KEY_ID','AWS_SECRET_ACCESS_KEY','AWS_SESSION_TOKEN',
            'GOOGLE_APPLICATION_CREDENTIALS','VERTEX_AI_PROJECT_ID') }
    )) {
        $handle = New-IsolatedTerminal
        Start-Sleep -Milliseconds 650
        $flags = @('set '+$entry.env+'='+$entry.home)
        foreach ($secret in $entry.clear) { $flags += ('set '+$secret+'=') }
        # cmd's child environment cannot persist into the Host shell.
        # The native CLI stays interactive on the existing isolated ConPTY.
        $shellLine = 'cmd /d /c "' + (($flags + $entry.command) -join '&& ') + '"'
        Send-TerminalCommand $handle $shellLine
        if (Wait-TerminalMatch $handle ([regex]::Escape($entry.marker)) 14) {
            Write-Output ($entry.name+' native historical content: PASS (isolated file was rendered)')
        } else {
            # Classify only the synthetic test output. Never echo a terminal
            # transcript or other raw provider text into the test log.
            $output = Read-TerminalText $handle
            $reason = if ($output -match '(?i)login|sign in|authenticate|auth.*required|api.key') {
                'isolated account not authenticated'
            } elseif ($output -match '(?i)no.saved.session|no.conversation.found|session.not.found') {
                'provider did not recognize fixture history format'
            } elseif ($output -match '(?i)not.recognized|not a terminal|stdin is not|unknown.option') {
                'installed CLI or ConPTY command issue'
            } else {
                'provider did not render synthetic history marker'
            }
            Write-Output ($entry.name+' native historical content: NOT VERIFIED ('+$reason+')')
        }
    }
}
