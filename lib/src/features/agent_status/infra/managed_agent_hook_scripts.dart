part of 'managed_agent_hook_installer.dart';

const List<String> _managedHookCompletionEvents = <String>[
  'Stop',
  'StopFailure',
  'SessionEnd',
  'Interrupt',
  'stop',
  'sessionEnd',
  'SessionIdle',
  'agent_settled',
  'session_shutdown',
  'agent.end',
  'ErrorOccurred',
];

String _managedHookPowerShellCompletionEvents() => _managedHookCompletionEvents
    .map((event) => "'${_powerShellSingleQuote(event)}'")
    .join(',');

extension _ManagedAgentHookScripts on ManagedAgentHookInstallService {
  String _managedCommand({
    required _AgentHookDescriptor descriptor,
    required _ManagedHookEvent event,
  }) {
    if (_platform == ManagedAgentHookPlatform.posix) {
      return 'if [ -x ${_shQuote(descriptor.scriptPath)} ]; then '
          '${descriptor.eventEnvVar}=${_shQuote(event.eventName)} '
          '/bin/sh ${_shQuote(descriptor.scriptPath)}; fi';
    }

    final customCommand = descriptor.windowsCommandsByEvent[event.eventName];
    if (customCommand != null) {
      return customCommand;
    }
    return switch (descriptor.windowsExecutionStrategy) {
      _WindowsHookExecutionStrategy.powerShell =>
        '\$env:${descriptor.eventEnvVar} = \'${_powerShellSingleQuote(event.eventName)}\'; '
            'powershell.exe -NoProfile -ExecutionPolicy Bypass -File '
            '${_powerShellPath(descriptor.scriptPath)}',
      _WindowsHookExecutionStrategy.gitBashToCmd =>
        // Devin launches hook commands through Git Bash on Windows. Keep `call`
        // and the batch path as separate argv: embedding quoted path text inside
        // one `/c` argument makes MSYS pass literal backslash-quote pairs to cmd.
        'if [ -f ${_shQuote(descriptor.scriptPath)} ]; then '
            "MSYS2_ARG_CONV_EXCL='*' "
            '${descriptor.eventEnvVar}=${_shQuote(event.eventName)} '
            'cmd.exe /d /s /c call ${_shQuote(descriptor.scriptPath)}; fi',
      _WindowsHookExecutionStrategy.nativeCmd =>
        'cmd /d /s /c "if exist ""${descriptor.scriptPath}"" '
            '(set ${descriptor.eventEnvVar}=${event.eventName}&& call ""${descriptor.scriptPath}"")"',
    };
  }

  // coverage:ignore-start
  // External shell/cmd hook templates. Installer tests verify selection and
  // persistence; exercising each literal line belongs to agent CLI smoke tests.
  String _managedScript({required _AgentHookDescriptor descriptor}) {
    final customBuilder = descriptor.managedScriptBuilder;
    if (customBuilder != null) {
      return customBuilder(this, descriptor);
    }
    final source = descriptor.agentType.key;
    final eventEnvVar = descriptor.eventEnvVar;
    if (_platform == ManagedAgentHookPlatform.windows) {
      if (descriptor.windowsExecutionStrategy ==
          _WindowsHookExecutionStrategy.powerShell) {
        return _windowsPowerShellManagedScript(
          source: source,
          eventEnvVar: eventEnvVar,
          writeEmptyResponse: descriptor.writeEmptyResponse,
        );
      }
      return <String>[
        '@echo off',
        'setlocal',
        'if defined ALERA_AGENT_HOOK_ENDPOINT if exist "%ALERA_AGENT_HOOK_ENDPOINT%" call "%ALERA_AGENT_HOOK_ENDPOINT%" 2>nul',
        'if "%ALERA_AGENT_HOOK_PORT%"=="" exit /b 0',
        'if "%ALERA_AGENT_HOOK_TOKEN%"=="" exit /b 0',
        'if "%ALERA_TERMINAL_SESSION_ID%"=="" exit /b 0',
        'if "%ALERA_WORKSPACE_ID%"=="" exit /b 0',
        'if "%ALERA_TAB_ID%"=="" exit /b 0',
        _windowsPostCommand(source, eventEnvVar),
        'exit /b 0',
        '',
      ].join('\r\n');
    }
    return <String>[
      '#!/bin/sh',
      if (descriptor.writeEmptyResponse) "printf '{}\\n'",
      'if [ -n "\$ALERA_AGENT_HOOK_ENDPOINT" ] && [ -r "\$ALERA_AGENT_HOOK_ENDPOINT" ]; then',
      '  . "\$ALERA_AGENT_HOOK_ENDPOINT" 2>/dev/null || :',
      'fi',
      'if [ -z "\$ALERA_AGENT_HOOK_PORT" ] || [ -z "\$ALERA_AGENT_HOOK_TOKEN" ] || [ -z "\$ALERA_TERMINAL_SESSION_ID" ] || [ -z "\$ALERA_WORKSPACE_ID" ] || [ -z "\$ALERA_TAB_ID" ]; then',
      '  exit 0',
      'fi',
      'payload=\$(cat)',
      'alera_completion_event=0',
      'case "\${$eventEnvVar}" in',
      '  ${_managedHookCompletionEvents.join('|')}) alera_completion_event=1 ;;',
      'esac',
      'if [ -z "\$payload" ]; then',
      '  if [ "\$alera_completion_event" -eq 1 ]; then payload=\'{}\'; else exit 0; fi',
      'fi',
      'alera_attempts=1',
      'if [ "\$alera_completion_event" -eq 1 ]; then alera_attempts=3; fi',
      'alera_attempt=1',
      'while [ "\$alera_attempt" -le "\$alera_attempts" ]; do',
      '  if curl -fsS -X POST "http://127.0.0.1:\${ALERA_AGENT_HOOK_PORT}/hook/$source" \\',
      '    --connect-timeout 0.25 --max-time 1.0 \\',
      '    -H "Content-Type: application/x-www-form-urlencoded" \\',
      '    -H "$aleraAgentHookTokenHeader: \${ALERA_AGENT_HOOK_TOKEN}" \\',
      '    --data-urlencode "terminalSessionId=\${ALERA_TERMINAL_SESSION_ID}" \\',
      '    --data-urlencode "workspaceId=\${ALERA_WORKSPACE_ID}" \\',
      '    --data-urlencode "tabId=\${ALERA_TAB_ID}" \\',
      '    --data-urlencode "hookEventName=\${$eventEnvVar}" \\',
      '    --data-urlencode "version=\${ALERA_AGENT_HOOK_VERSION}" \\',
      '    --data-urlencode "payload=\${payload}" >/dev/null 2>&1; then',
      '    break',
      '  fi',
      '  if [ "\$alera_attempt" -lt "\$alera_attempts" ]; then',
      '    case "\$alera_attempt" in 1) sleep 0.05 ;; *) sleep 0.10 ;; esac',
      '  fi',
      '  alera_attempt=\$((alera_attempt + 1))',
      'done',
      'exit 0',
      '',
    ].join('\n');
  }
  // coverage:ignore-end

  String _windowsPostCommand(
    String source,
    String eventEnvVar, {
    String? emptyPayloadFallbackEvent,
    bool allowEmptyPayload = false,
  }) {
    final completionScript =
        '\$completionEvents=@(${_managedHookPowerShellCompletionEvents()}); \$completion=\$completionEvents -contains \$env:$eventEnvVar;';
    final inputScript = allowEmptyPayload
        ? 'if ([string]::IsNullOrWhiteSpace(\$inputData)) { \$payload=@{} } else { \$payload=(\$inputData | ConvertFrom-Json) };'
        : emptyPayloadFallbackEvent == null
        ? 'if ([string]::IsNullOrWhiteSpace(\$inputData)) { if (\$completion) { \$payload=@{} } else { exit 0 } } else { \$payload=(\$inputData | ConvertFrom-Json) };'
        : 'if ([string]::IsNullOrWhiteSpace(\$inputData)) { if (\$completion -or \$env:$eventEnvVar -ieq \'${_powerShellSingleQuote(emptyPayloadFallbackEvent)}\') { \$payload=@{} } else { exit 0 } } else { \$payload=(\$inputData | ConvertFrom-Json) };';
    return 'powershell -NoProfile -ExecutionPolicy Bypass -Command "\$utf8=[System.Text.UTF8Encoding]::new(\$false); [Console]::InputEncoding=\$utf8; [Console]::OutputEncoding=\$utf8; \$inputData=[Console]::In.ReadToEnd(); $completionScript $inputScript try { \$body=@{ terminalSessionId=\$env:ALERA_TERMINAL_SESSION_ID; workspaceId=\$env:ALERA_WORKSPACE_ID; tabId=\$env:ALERA_TAB_ID; hookEventName=\$env:$eventEnvVar; version=\$env:ALERA_AGENT_HOOK_VERSION; payload=\$payload } | ConvertTo-Json -Depth 100 -Compress; \$bodyBytes=\$utf8.GetBytes(\$body) } catch { exit 0 }; \$attempts=if (\$completion) { 3 } else { 1 }; for (\$attempt=1; \$attempt -le \$attempts; \$attempt++) { try { Invoke-WebRequest -UseBasicParsing -TimeoutSec 1 -Method Post -Uri (\'http://127.0.0.1:\' + \$env:ALERA_AGENT_HOOK_PORT + \'/hook/$source\') -ContentType \'application/json; charset=utf-8\' -Headers @{ \'$aleraAgentHookTokenHeader\'=\$env:ALERA_AGENT_HOOK_TOKEN } -Body \$bodyBytes | Out-Null; break } catch {} if (\$attempt -lt \$attempts) { Start-Sleep -Milliseconds (50 * \$attempt) } }"';
  }

  Map<String, Object?> _managedHookDefinition(
    _AgentHookDescriptor descriptor,
    _ManagedHookEvent event,
    String command,
  ) {
    final shape = event.definitionShape ?? descriptor.definitionShape;
    return switch (shape) {
      _ManagedHookDefinitionShape.nestedCommand => <String, Object?>{
        if (event.matcher != null) 'matcher': event.matcher,
        'hooks': <Object?>[
          <String, Object?>{'type': 'command', 'command': command},
        ],
      },
      _ManagedHookDefinitionShape.directCommand => <String, Object?>{
        'type': 'command',
        if (_platform == ManagedAgentHookPlatform.windows)
          'powershell': command
        else
          'bash': command,
        'timeoutSec': 5,
      },
      _ManagedHookDefinitionShape.topLevelCommand => <String, Object?>{
        'command': command,
      },
      _ManagedHookDefinitionShape.agyLifecycleCommand => <String, Object?>{
        'type': 'command',
        'command': command,
        'timeout': 10,
      },
      _ManagedHookDefinitionShape.agyToolCommand => <String, Object?>{
        if (event.matcher != null) 'matcher': event.matcher,
        'hooks': <Object?>[
          <String, Object?>{
            'type': 'command',
            'command': command,
            'timeout': 10,
          },
        ],
      },
    };
  }

  Map<String, Object?> _hookContainer(
    Map<String, Object?> config,
    _AgentHookDescriptor descriptor,
  ) {
    return switch (descriptor.configShape) {
      _AgentHookConfigShape.hooks => _hooksMap(config),
      _AgentHookConfigShape.agyBundle => _mapFromValue(
        config[descriptor.bundleName],
      ),
    };
  }

  void _setHookContainer(
    Map<String, Object?> config,
    _AgentHookDescriptor descriptor,
    Map<String, Object?> hooks,
  ) {
    switch (descriptor.configShape) {
      case _AgentHookConfigShape.hooks:
        config['hooks'] = hooks;
      case _AgentHookConfigShape.agyBundle:
        if (hooks.isEmpty) {
          config.remove(descriptor.bundleName);
        } else {
          config[descriptor.bundleName] = hooks;
        }
    }
  }
}
