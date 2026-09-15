part of 'codex_runtime_home_service_test.dart';

void _registerCodexRuntimeHomeServiceAdvancedTests() {
  test(
    'uses native resume state directly without creating runtime links',
    () async {
      final sessions = Directory(p.join(home.path, '.codex', 'sessions'))
        ..createSync(recursive: true);
      final history = File(p.join(home.path, '.codex', 'history.jsonl'))
        ..writeAsStringSync('{"session":"native"}\n');
      var linkAttempts = 0;
      final directService = CodexRuntimeHomeService(
        homeDirectory: home.path,
        applicationSupportDirectory: () async => support,
        platform: .posix,
        environment: <String, String>{'HOME': home.path},
        resourceLinkCreator: ({required sourcePath, required targetPath}) {
          linkAttempts += 1;
        },
      );

      final preparation = await directService.prepareForTerminalLaunch();

      expect(preparation.runtimeHomePath, p.join(home.path, '.codex'));
      expect(preparation.environment, isEmpty);
      expect(linkAttempts, 0);
      expect(sessions.existsSync(), isTrue);
      expect(history.readAsStringSync(), '{"session":"native"}\n');
      expect(
        Directory(p.join(support.path, 'agent-runtime-homes', 'codex'))
            .existsSync(),
        isFalse,
      );
    },
  );

  test(
    'uses an explicit CODEX_HOME directly without an Alera overlay',
    () async {
      final customHome = Directory(p.join(root.path, 'custom-codex'))
        ..createSync(recursive: true);
      final customSession = File(p.join(customHome.path, 'history.jsonl'))
        ..writeAsStringSync('{"session":"custom"}\n');
      final customService = CodexRuntimeHomeService(
        homeDirectory: home.path,
        applicationSupportDirectory: () async => support,
        platform: .posix,
        environment: <String, String>{
          'HOME': home.path,
          'CODEX_HOME': customHome.path,
        },
        resourceLinkCreator: ({required sourcePath, required targetPath}) {
          fail('direct custom CODEX_HOME must not create runtime links');
        },
      );

      final preparation = await customService.prepareForTerminalLaunch();

      expect(preparation.runtimeHomePath, customHome.path);
      expect(preparation.environment, isEmpty);
      expect(customSession.readAsStringSync(), '{"session":"custom"}\n');
      expect(File(p.join(customHome.path, 'hooks.json')).existsSync(), isTrue);
      expect(File(p.join(customHome.path, 'config.toml')).existsSync(), isTrue);
      expect(
        File(p.join(home.path, '.codex', 'hooks.json')).existsSync(),
        isFalse,
      );
    },
  );

  test('removes mirrored auth when system auth disappears', () async {
    final systemAuth = File(p.join(home.path, '.codex', 'auth.json'))
      ..createSync(recursive: true)
      ..writeAsStringSync('{"token":"system"}\n');

    final preparation = await service.prepareForTerminalLaunch();
    final runtimeAuth = File(p.join(preparation.runtimeHomePath, 'auth.json'));

    expect(runtimeAuth.existsSync(), isTrue);
    expect(runtimeAuth.readAsStringSync(), systemAuth.readAsStringSync());

    systemAuth.deleteSync();
    await service.prepareForTerminalLaunch();

    expect(
      FileSystemEntity.typeSync(runtimeAuth.path, followLinks: false),
      FileSystemEntityType.notFound,
    );
  });

  test('mirrors trusted user hook state into the runtime config', () async {
    final systemHooksPath = p.join(home.path, '.codex', 'hooks.json');
    const userCommand = 'echo trusted-user-hook';
    _writeJson(systemHooksPath, <String, Object?>{
      'hooks': <String, Object?>{
        'PreToolUse': <Object?>[_userHook(userCommand)],
      },
    });
    final canonicalSystemHooksPath = File(systemHooksPath)
        .resolveSymbolicLinksSync();
    final systemTrustKey = '$canonicalSystemHooksPath:pre_tool_use:0:0';
    final systemTrustedHash = computeCodexTrustedHashForTesting(
      sourcePath: systemHooksPath,
      eventLabel: 'pre_tool_use',
      groupIndex: 0,
      handlerIndex: 0,
      command: userCommand,
    );
    File(p.join(home.path, '.codex', 'config.toml'))
      ..createSync(recursive: true)
      ..writeAsStringSync(
        '[hooks.state."${_escapeTomlString(systemTrustKey)}"]\n'
        'enabled = false\n'
        'trusted_hash = "$systemTrustedHash"\n',
      );

    final preparation = await service.prepareForTerminalLaunch();

    final runtimeHooksPath = p.join(preparation.runtimeHomePath, 'hooks.json');
    final canonicalRuntimeHooksPath = File(runtimeHooksPath)
        .resolveSymbolicLinksSync();
    final runtimeTrustKey = '$canonicalRuntimeHooksPath:pre_tool_use:0:0';
    final runtimeToml = File(p.join(preparation.runtimeHomePath, 'config.toml'))
        .readAsStringSync();
    expect(
      runtimeToml,
      contains('[hooks.state."${_escapeTomlString(runtimeTrustKey)}"]'),
    );
    expect(runtimeToml, contains('enabled = false'));
    expect(runtimeToml, contains('trusted_hash = "$systemTrustedHash"'));
  });

  test(
    'parses escaped TOML trust strings while checking runtime status',
    () async {
      final preparation = await service.prepareForTerminalLaunch();
      final runtimeTomlPath = p.join(
        preparation.runtimeHomePath,
        'config.toml',
      );
      File(runtimeTomlPath).writeAsStringSync(r'''
[hooks.state."escaped\n\r\t\b\f\"\\z:stop:0:0"]
enabled = true
trusted_hash = "sha256:escaped\n\r\t\b\f\"\\z"
''', mode: .append);

      final status = await service.status();

      expect(status.state, ManagedAgentHookInstallState.installed);
    },
  );

  test('parses unknown TOML escapes and EOF trust blocks', () async {
    final preparation = await service.prepareForTerminalLaunch();
    final runtimeTomlPath = p.join(preparation.runtimeHomePath, 'config.toml');
    File(runtimeTomlPath).writeAsStringSync(
      '\n[hooks.state."unknown\\q:stop:0:0"]\n'
      'enabled = true',
      mode: .append,
    );

    final status = await service.status();

    expect(status.state, ManagedAgentHookInstallState.installed);
  });

  test(
    'preserves unrelated trust entries when managed hooks refresh',
    () async {
      final preparation = await service.prepareForTerminalLaunch();
      final runtimeHooksPath = p.join(
        preparation.runtimeHomePath,
        'hooks.json',
      );
      final canonicalRuntimeHooksPath = File(runtimeHooksPath)
          .resolveSymbolicLinksSync();
      final runtimeTomlPath = p.join(
        preparation.runtimeHomePath,
        'config.toml',
      );
      File(runtimeTomlPath).writeAsStringSync(
        _trustBlock(
          key: '$canonicalRuntimeHooksPath:stop:99:99',
          enabled: true,
          trustedHash: 'sha256:stale',
        ),
        mode: .append,
      );

      await service.prepareForTerminalLaunch();

      expect(
        File(runtimeTomlPath).readAsStringSync(),
        contains('sha256:stale'),
      );
    },
  );

  test('preserves hook-like text inside multiline TOML strings', () async {
    final systemConfig = File(p.join(home.path, '.codex', 'config.toml'))
      ..createSync(recursive: true)
      ..writeAsStringSync(
        'note = """\n'
        '[hooks.state."fake-system"]\n'
        'trusted_hash = "not-a-section"\n'
        '"""\n'
        '\n'
        '[features]\n'
        'codex_hooks = true\n',
      );
    final preparation = await service.prepareForTerminalLaunch();

    final runtimeToml = File(p.join(preparation.runtimeHomePath, 'config.toml'))
        .readAsStringSync();
    expect(runtimeToml, contains('[hooks.state."fake-system"]'));
    expect(runtimeToml, contains('trusted_hash = "not-a-section"'));
    expect(runtimeToml, contains('hooks = true'));
    expect(runtimeToml, isNot(contains('codex_hooks')));
    expect(
      systemConfig.readAsStringSync(),
      contains('[hooks.state."fake-system"]'),
    );
    expect(
      systemConfig.readAsStringSync(),
      contains('trusted_hash = "not-a-section"'),
    );
    expect(systemConfig.readAsStringSync(), contains('hooks = true'));
  });

  test(
    'normalizes complex TOML headers without treating strings as tables',
    () async {
      File(p.join(home.path, '.codex', 'config.toml'))
        ..createSync(recursive: true)
        ..writeAsStringSync(
          '\ufefftitle = "demo"\n'
          '\n'
          '[features]\n'
          'basic_note = """line with escaped \\\\" quote\n'
          '[not.a.table]\n'
          '"""\n'
          "literal_note = '''\n"
          '[also.not.a.table]\n'
          "'''\n"
          'codex_hooks = false\n'
          'hooks = false\n'
          '\n'
          '[projects."escaped\\\\"project"]\n'
          'trust_level = "trusted"\n'
          '\n'
          "[projects.'literal]project']\n"
          'trust_level = "trusted"\n'
          '\n'
          '[[profiles."array]profile"]] # valid array table\n'
          'name = "demo"\n'
          '\n'
          '[[broken.array] trailing text\n',
        );

      final preparation = await service.prepareForTerminalLaunch();

      final runtimeToml = File(
        p.join(preparation.runtimeHomePath, 'config.toml'),
      ).readAsStringSync();
      expect(runtimeToml.codeUnitAt(0), isNot(0xfeff));
      expect(runtimeToml, contains('hooks = true'));
      expect(runtimeToml, isNot(contains('codex_hooks')));
      expect(runtimeToml, contains('[not.a.table]'));
      expect(runtimeToml, contains('[also.not.a.table]'));
      expect(runtimeToml, contains('[projects."escaped\\\\"project"]'));
      expect(runtimeToml, contains("[projects.'literal]project']"));
      expect(runtimeToml, contains('[[profiles."array]profile"]]'));
      expect(runtimeToml, contains('[[broken.array] trailing text'));
    },
  );

  test('ignores fake trust blocks inside multiline TOML strings', () async {
    final systemHooksPath = p.join(home.path, '.codex', 'hooks.json');
    const userCommand = 'echo fake-trusted-user-hook';
    _writeJson(systemHooksPath, <String, Object?>{
      'hooks': <String, Object?>{
        'PreToolUse': <Object?>[_userHook(userCommand)],
      },
    });
    final canonicalSystemHooksPath = File(systemHooksPath)
        .resolveSymbolicLinksSync();
    final systemTrustKey = '$canonicalSystemHooksPath:pre_tool_use:0:0';
    final systemTrustedHash = computeCodexTrustedHashForTesting(
      sourcePath: systemHooksPath,
      eventLabel: 'pre_tool_use',
      groupIndex: 0,
      handlerIndex: 0,
      command: userCommand,
    );
    File(p.join(home.path, '.codex', 'config.toml'))
      ..createSync(recursive: true)
      ..writeAsStringSync(
        'note = """\n'
        '[hooks.state."${_escapeTomlString(systemTrustKey)}"]\n'
        'enabled = false\n'
        'trusted_hash = "$systemTrustedHash"\n'
        '"""\n',
      );

    final preparation = await service.prepareForTerminalLaunch();

    final runtimeHooksPath = p.join(preparation.runtimeHomePath, 'hooks.json');
    final canonicalRuntimeHooksPath = File(runtimeHooksPath)
        .resolveSymbolicLinksSync();
    final managedTrustKey = '$canonicalRuntimeHooksPath:pre_tool_use:1:0';
    final runtimeToml = File(p.join(preparation.runtimeHomePath, 'config.toml'))
        .readAsStringSync();
    expect(
      runtimeToml,
      contains('[hooks.state."${_escapeTomlString(systemTrustKey)}"]'),
    );
    expect(
      runtimeToml,
      contains('[hooks.state."${_escapeTomlString(managedTrustKey)}"]'),
    );
  });

  test(
    'resolves Codex home from USERPROFILE and current directory fallbacks',
    () {
      final profileService = CodexRuntimeHomeService(
        applicationSupportDirectory: () async => support,
        platform: .posix,
        environment: <String, String>{'HOME': '', 'USERPROFILE': home.path},
      );
      final currentFallbackService = CodexRuntimeHomeService(
        applicationSupportDirectory: () async => support,
        platform: .posix,
        environment: const <String, String>{},
      );

      expect(profileService, isA<CodexRuntimeHomeService>());
      expect(currentFallbackService, isA<CodexRuntimeHomeService>());
    },
  );
}
