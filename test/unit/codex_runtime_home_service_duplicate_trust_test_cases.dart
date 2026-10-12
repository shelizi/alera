part of 'codex_runtime_home_service_test.dart';

void _registerCodexRuntimeHomeServiceDuplicateTrustTests() {
  test('reconciles shared runtime and legacy Codex hooks together', () async {
    final windowsService = CodexRuntimeHomeService(
      homeDirectory: home.path,
      applicationSupportDirectory: () async => support,
      platform: .windows,
      environment: <String, String>{'USERPROFILE': home.path},
    );
    final hooksPath = p.join(home.path, '.codex', 'hooks.json');
    _writeJson(hooksPath, <String, Object?>{
      'hooks': <String, Object?>{
        'PreToolUse': <Object?>[
          _userHook('echo user-hook'),
          _userHook('call "C:\\old\\alera-runtime-agent-hook.cmd"'),
          _userHook('if [ -f alera-codex-hook.cmd ]; then echo old; fi'),
        ],
      },
    });
    File(p.join(home.path, '.codex', 'config.toml')).writeAsStringSync(
      _trustBlock(
        key: '$hooksPath:pre_tool_use:1:0',
        enabled: false,
        trustedHash: 'sha256:wrong-installer',
      ),
    );
    await windowsService.prepareForTerminalLaunch();
    await windowsService.prepareForTerminalLaunch();
    final commands = _commandsFor(_hooks(hooksPath), 'PreToolUse');
    expect(commands, hasLength(2));
    expect(commands.first, 'echo user-hook');
    expect(commands.last, startsWith('cmd /d /s /c '));
    expect(commands.join(), isNot(contains('alera-runtime-agent-hook.cmd')));
    expect(commands.join(), isNot(contains('if [')));
    final toml = File(p.join(home.path, '.codex', 'config.toml'));
    final parsed = TomlDocument.parse(toml.readAsStringSync()).toMap();
    expect((parsed['hooks'] as Map)['state'], hasLength(8));
    expect(
      (await windowsService.status()).state,
      ManagedAgentHookInstallState.installed,
    );
    await windowsService.remove();
    expect(_commandsFor(_hooks(hooksPath), 'PreToolUse'), <String>[
      'echo user-hook',
    ]);
  });

  test('Windows Codex commands execute with spaced batch paths', () async {
    if (!Platform.isWindows) {
      return;
    }
    final spacedHome = Directory(p.join(root.path, "user space (dev)'s"))
      ..createSync();
    final windowsService = CodexRuntimeHomeService(
      homeDirectory: spacedHome.path,
      applicationSupportDirectory: () async => support,
      platform: .windows,
      environment: <String, String>{'USERPROFILE': spacedHome.path},
    );
    final preparation = await windowsService.prepareForTerminalLaunch();
    final script = File(
      p.join(spacedHome.path, '.alera', 'agent-hooks', 'alera-codex-hook.cmd'),
    );
    script.writeAsStringSync(
      '@echo off\r\necho %ALERA_AGENT_HOOK_EVENT%\r\nexit /b 0\r\n',
    );
    final hooks = _hooks(p.join(preparation.runtimeHomePath, 'hooks.json'));
    for (final event in <String>['UserPromptSubmit', 'PreToolUse', 'Stop']) {
      final probe = File(p.join(root.path, 'probe.cmd'));
      probe.writeAsStringSync(
        '@echo off\r\n${_commandsFor(hooks, event).single}\r\n',
      );
      final result = await Process.run('cmd.exe', <String>[
        '/d',
        '/c',
        probe.path,
      ]);
      expect(result.exitCode, 0, reason: '${result.stderr}');
      expect((result.stdout as String).trim(), event);
    }
  });

  for (final platform in ManagedAgentHookPlatform.values) {
    test('repairs duplicate semantic trust keys on $platform', () async {
      final localService = CodexRuntimeHomeService(
        homeDirectory: home.path,
        applicationSupportDirectory: () async => support,
        platform: platform,
        environment: <String, String>{'HOME': home.path},
      );
      final preparation = await localService.prepareForTerminalLaunch();
      final file = File(p.join(preparation.runtimeHomePath, 'config.toml'));
      final original = file.readAsStringSync();
      final literal = _literalTrustHeaders(original);
      final duplicateBlocks = literal.substring(
        literal.indexOf('[hooks.state.'),
      );
      final escapedBlocks = original
          .substring(original.indexOf('[hooks.state.'))
          .replaceAll(
            'hooks.json',
            platform == ManagedAgentHookPlatform.windows
                ? r'\U00000068ooks.json'
                : r'\u0068ooks.json',
          );
      const userBlock =
          '[hooks.state."user:stop:99:0"]\n'
          'enabled = false\ntrusted_hash = "sha256:user"\n';
      final managedHeader = original
          .split('\n')
          .firstWhere((line) => line.startsWith('[hooks.state.'));
      final note = "note = '''\n$managedHeader\n'''\n";
      var corrupted =
          '$note$original\n$duplicateBlocks\n$escapedBlocks\n$userBlock';
      if (platform == ManagedAgentHookPlatform.windows) {
        corrupted = corrupted.replaceAll('\n', '\r\n');
      }
      file.writeAsStringSync(corrupted);

      await localService.prepareForTerminalLaunch();
      final repaired = file.readAsStringSync();
      final parsed = TomlDocument.parse(repaired).toMap();
      final states = (parsed['hooks'] as Map)['state'] as Map;
      expect(states, hasLength(9));
      expect(states['user:stop:99:0']['enabled'], false);
      expect(parsed['note'], contains(managedHeader));
      final hooks = _hooks(p.join(preparation.runtimeHomePath, 'hooks.json'));
      for (final event in <String>[
        'SessionStart',
        'UserPromptSubmit',
        'PreToolUse',
        'PermissionRequest',
        'PostToolUse',
        'Stop',
        'Interrupt',
        'SessionEnd',
      ]) {
        expect(_commandsFor(hooks, event), hasLength(1));
      }

      file.setLastModifiedSync(DateTime(2020));
      final modified = file.lastModifiedSync();
      await localService.prepareForTerminalLaunch();
      expect(file.readAsStringSync(), repaired);
      expect(file.lastModifiedSync(), modified);
      expect(
        (await localService.status()).state,
        ManagedAgentHookInstallState.installed,
      );
      await localService.remove();
      final remaining = TomlDocument.parse(file.readAsStringSync()).toMap();
      expect((remaining['hooks'] as Map)['state'], hasLength(1));
    });
  }
}
