part of 'managed_agent_hook_installer_test.dart';

void _registerCursorHookInstallerTests(
  Directory Function() readHome,
  ManagedAgentHookInstallService Function() readService,
) {
  test('installs Cursor hooks in user config and preserves user hooks', () {
    final configPath = p.join(readHome().path, '.cursor', 'hooks.json');
    _writeJson(configPath, <String, Object?>{
      'version': 1,
      'custom': true,
      'hooks': <String, Object?>{
        'stop': <Object?>[
          <String, Object?>{'command': 'audit.sh'},
          <String, Object?>{
            'command':
                '/home/user/.alera/agent-hooks/alera-runtime-agent-hook.sh',
          },
        ],
      },
    });
    expect(
      readService().install(.cursor).state,
      ManagedAgentHookInstallState.installed,
    );
    final config = _readJson(configPath);
    final hooks = config['hooks'] as Map<String, Object?>;
    expect(hooks, isNot(contains('sessionStart')));
    expect((hooks['stop'] as List).length, 2);
    expect((hooks['stop'] as List).last, containsPair('timeout', 5));
    final before = File(configPath).readAsStringSync();
    readService().install(.cursor);
    expect(File(configPath).readAsStringSync(), before);
    expect(
      readService().status(.cursor).state,
      ManagedAgentHookInstallState.installed,
    );
    expect(
      readService().remove(.cursor).state,
      ManagedAgentHookInstallState.notInstalled,
    );
    final cleaned = _readJson(configPath);
    expect(cleaned['custom'], true);
    expect((cleaned['hooks'] as Map)['stop'], <Object?>[
      <String, Object?>{'command': 'audit.sh'},
    ]);
  });

  test('installs Cursor hooks using the native Windows command', () {
    final windowsService = ManagedAgentHookInstallService(
      homeDirectory: readHome().path,
      platform: .windows,
      environment: <String, String>{'USERPROFILE': readHome().path},
    );
    expect(
      windowsService.install(.cursor).state,
      ManagedAgentHookInstallState.installed,
    );
    final hooks = _hooks(p.join(readHome().path, '.cursor', 'hooks.json'));
    final command = _commandsFor(hooks, 'stop').single;
    expect(command, contains('cmd /d /s /c'));
    expect(command, contains('ALERA_CURSOR_EVENT=stop'));
    expect(command, isNot(contains('--plugin-dir')));
  });

  test('preserves invalid Cursor config when installation fails', () {
    final file = File(p.join(readHome().path, '.cursor', 'hooks.json'))
      ..createSync(recursive: true);
    file.writeAsStringSync('{invalid');
    expect(
      readService().install(.cursor).state,
      ManagedAgentHookInstallState.error,
    );
    expect(file.readAsStringSync(), '{invalid');
  });
}
