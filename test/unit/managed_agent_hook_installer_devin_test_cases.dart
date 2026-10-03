part of 'managed_agent_hook_installer_test.dart';

void _registerDevinHookInstallerTests(
  Directory Function() home,
  ManagedAgentHookInstallService Function() service,
) {
  test('installs Devin hooks in the official user config without matchers', () {
    final configPath = p.join(home().path, '.config', 'devin', 'config.json');
    _writeJson(configPath, <String, Object?>{
      'model': 'existing-model',
      'hooks': <String, Object?>{
        'UserPromptSubmit': <Object?>[
          <String, Object?>{
            'hooks': <Object?>[
              <String, Object?>{'type': 'command', 'command': 'echo user-hook'},
            ],
          },
        ],
      },
    });

    final status = service().install(.devin);
    final config = _readJson(configPath);
    final hooks = Map<String, Object?>.from(config['hooks'] as Map);

    expect(status.state, ManagedAgentHookInstallState.installed);
    expect(config['model'], 'existing-model');
    expect(
      hooks.keys,
      containsAll(<String>[
        'SessionStart',
        'UserPromptSubmit',
        'PreToolUse',
        'PostToolUse',
        'PermissionRequest',
        'Stop',
        'SessionEnd',
      ]),
    );
    final preTool = Map<String, Object?>.from(
      (hooks['PreToolUse'] as List).single as Map,
    );
    expect(preTool.containsKey('matcher'), isFalse);
    expect(_commandsFor(hooks, 'UserPromptSubmit'), hasLength(2));
    expect(
      _commandsFor(hooks, 'UserPromptSubmit').last,
      contains('ALERA_DEVIN_EVENT'),
    );
    expect(
      File(p.join(home().path, '.alera', 'agent-hooks', 'alera-devin-hook.sh'))
          .readAsStringSync(),
      contains('/hook/devin'),
    );

    final removed = service().remove(.devin);
    final nextConfig = _readJson(configPath);
    final nextHooks = Map<String, Object?>.from(nextConfig['hooks'] as Map);
    expect(removed.state, ManagedAgentHookInstallState.notInstalled);
    expect(nextConfig['model'], 'existing-model');
    expect(_commandsFor(nextHooks, 'UserPromptSubmit'), <String>[
      'echo user-hook',
    ]);
    expect(nextHooks['PreToolUse'], isNull);
  });

  test('uses APPDATA for the Devin user config on Windows', () {
    final appData = p.join(home().path, 'AppData', 'Roaming');
    final windowsService = ManagedAgentHookInstallService(
      homeDirectory: home().path,
      platform: .windows,
      environment: <String, String>{
        'USERPROFILE': home().path,
        'APPDATA': appData,
      },
    );

    final status = windowsService.install(.devin);

    expect(status.state, ManagedAgentHookInstallState.installed);
    expect(status.configPath, p.join(appData, 'devin', 'config.json'));
  });

  test('bridges Devin Windows hooks from Git Bash into cmd', () {
    final appData = p.join(home().path, 'AppData', 'Roaming');
    final windowsService = ManagedAgentHookInstallService(
      homeDirectory: home().path,
      platform: .windows,
      environment: <String, String>{
        'USERPROFILE': home().path,
        'APPDATA': appData,
      },
    );

    windowsService.install(.devin);

    // Devin invokes hook commands through Git Bash on Windows, but the managed
    // hook itself can remain the normal Windows cmd script. Keep `call` and the
    // script path as separate argv so MSYS does not serialize embedded quotes
    // as literal backslash-quote pairs before cmd.exe sees them.
    final script = File(
      p.join(home().path, '.alera', 'agent-hooks', 'alera-devin-hook.cmd'),
    );
    expect(script.existsSync(), isTrue);
    final scriptSource = script.readAsStringSync();
    expect(scriptSource, contains('/hook/devin'));
    expect(scriptSource, isNot(contains('endpoint.env')));
    expect(
      scriptSource,
      contains(
        r'if ([string]::IsNullOrWhiteSpace($inputData)) { if ($completion) { $payload=@{} } else { exit 0 } }',
      ),
    );
    expect(
      scriptSource,
      contains(r'Start-Sleep -Milliseconds (50 * $attempt)'),
    );

    final config = _readJson(p.join(appData, 'devin', 'config.json'));
    final hooks = Map<String, Object?>.from(config['hooks'] as Map);
    final command = _commandsFor(hooks, 'Stop').single;
    expect(command, contains("MSYS2_ARG_CONV_EXCL='*'"));
    expect(command, contains("ALERA_DEVIN_EVENT='Stop'"));
    expect(command, contains('cmd.exe /d /s /c call '));
    expect(command, isNot(contains('/c \'if exist "')));
    expect(command, contains('alera-devin-hook.cmd'));
    expect(command, isNot(contains('alera-devin-hook.sh')));
  });
}
