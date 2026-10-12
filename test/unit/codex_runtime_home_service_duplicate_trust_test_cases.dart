part of 'codex_runtime_home_service_test.dart';

void _registerCodexRuntimeHomeServiceDuplicateTrustTests() {
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
