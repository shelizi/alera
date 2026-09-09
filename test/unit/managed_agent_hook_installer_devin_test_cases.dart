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
}
