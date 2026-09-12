part of 'terminal_runtime_native_test.dart';

void _registerTerminalRuntimeLaunchInputOwnerTests() {
  group('TerminalRuntimeLaunchInputOwner', () {
    test('candidateLaunches returns launches from builder', () {
      final launch = _launch('sh', shell: '/bin/sh');
      final owner = TerminalRuntimeLaunchInputOwner(
        shellLaunchesBuilder: () => <GhosttyTerminalShellLaunch>[launch],
        clipboard: _FakeTerminalClipboard(),
        onOsc52Blocked: () {},
      );

      expect(owner.candidateLaunches(), equals(<GhosttyTerminalShellLaunch>[launch]));
    });

    test('buildAgentHookEnvironment invokes builder with tab details', () async {
      final owner = TerminalRuntimeLaunchInputOwner(
        shellLaunchesBuilder: () => const <GhosttyTerminalShellLaunch>[],
        agentHookEnvironmentBuilder: ({
          required terminalSessionId,
          required workspaceId,
          required tabId,
        }) async {
          return <String, String>{'SESSION': terminalSessionId};
        },
        clipboard: _FakeTerminalClipboard(),
        onOsc52Blocked: () {},
      );

      final env = await owner.buildAgentHookEnvironment(
        terminalSessionId: 'sess-1',
        workspaceId: 'ws-1',
        tabId: 'tab-1',
      );

      expect(env, equals(<String, String>{'SESSION': 'sess-1'}));
    });

    test('prepareLaunch applies working directory and login shell', () async {
      final launch = GhosttyTerminalShellLaunch(
        label: 'zsh',
        shell: '/bin/zsh',
        arguments: const <String>[],
        environment: const <String, String>{'FOO': 'bar'},
      );
      final owner = TerminalRuntimeLaunchInputOwner(
        shellLaunchesBuilder: () => <GhosttyTerminalShellLaunch>[launch],
        clipboard: _FakeTerminalClipboard(),
        onOsc52Blocked: () {},
      );

      final prepared = await owner.prepareLaunch(
        launch: launch,
        workspacePath: '/tmp/ws',
        resolvedLoginShell: true,
        agentHookEnvironment: const <String, String>{'HOOK': '1'},
      );

      expect(prepared.arguments, contains('-l'));
      expect(prepared.environment['HOOK'], '1');
    });

    test('onProcessCreated calls process created and delivers startup', () async {
      String? createdSessionId;
      final session = _FakeTerminalPtySession();
      final launch = GhosttyTerminalShellLaunch(
        label: 'sh',
        shell: '/bin/sh',
        arguments: const <String>[],
        environment: const <String, String>{},
        setupCommand: 'echo ready\n',
      );
      final owner = TerminalRuntimeLaunchInputOwner(
        shellLaunchesBuilder: () => <GhosttyTerminalShellLaunch>[launch],
        terminalProcessCreated: (id) => createdSessionId = id,
        clipboard: _FakeTerminalClipboard(),
        onOsc52Blocked: () {},
      );

      await owner.onProcessCreated(
        terminalSessionId: 'sess-99',
        session: session,
        workspaceLaunch: launch,
        interactiveShell: '/bin/sh',
        initialCommand: null,
        isCurrent: () => true,
      );

      expect(createdSessionId, 'sess-99');
      expect(session.writes.map(utf8.decode).join(), contains('echo ready\n'));
    });

    test('notifyInteraction invokes callback', () {
      final notices = <String>[];
      final owner = TerminalRuntimeLaunchInputOwner(
        shellLaunchesBuilder: () => const <GhosttyTerminalShellLaunch>[],
        interactionNotice: (msg, {error = false}) => notices.add(msg),
        clipboard: _FakeTerminalClipboard(),
        onOsc52Blocked: () {},
      );

      owner.notifyInteraction('hello');
      expect(notices, equals(<String>['hello']));
    });

    test('notifyOsc52Blocked invokes callback', () {
      var blockedCalled = false;
      final owner = TerminalRuntimeLaunchInputOwner(
        shellLaunchesBuilder: () => const <GhosttyTerminalShellLaunch>[],
        clipboard: _FakeTerminalClipboard(),
        onOsc52Blocked: () => blockedCalled = true,
      );

      owner.notifyOsc52Blocked();
      expect(blockedCalled, isTrue);
    });

    test('storeClipboard writes to clipboard when permitted', () async {
      final clipboard = _FakeTerminalClipboard();
      final owner = TerminalRuntimeLaunchInputOwner(
        shellLaunchesBuilder: () => const <GhosttyTerminalShellLaunch>[],
        clipboard: clipboard,
        onOsc52Blocked: () {},
      );

      owner.storeClipboard(text: 'copied text', allowOsc52Clipboard: true);
      await Future<void>.delayed(Duration.zero);

      expect(clipboard.writes, equals(<String>['copied text']));
    });

    test('storeClipboard triggers onOsc52Blocked when not allowed', () {
      final clipboard = _FakeTerminalClipboard();
      var blockedCalled = false;
      final owner = TerminalRuntimeLaunchInputOwner(
        shellLaunchesBuilder: () => const <GhosttyTerminalShellLaunch>[],
        clipboard: clipboard,
        onOsc52Blocked: () => blockedCalled = true,
      );

      owner.storeClipboard(text: 'blocked', allowOsc52Clipboard: false);

      expect(blockedCalled, isTrue);
      expect(clipboard.writes, isEmpty);
    });

    test('storeClipboard does nothing when disposed', () {
      final clipboard = _FakeTerminalClipboard();
      var blockedCalled = false;
      final owner = TerminalRuntimeLaunchInputOwner(
        shellLaunchesBuilder: () => const <GhosttyTerminalShellLaunch>[],
        clipboard: clipboard,
        onOsc52Blocked: () => blockedCalled = true,
      );

      owner.storeClipboard(
        text: 'ignored',
        allowOsc52Clipboard: true,
        isDisposed: true,
      );

      expect(blockedCalled, isFalse);
      expect(clipboard.writes, isEmpty);
    });

    test('pasteClipboard pastes text when available', () async {
      final clipboard = _FakeTerminalClipboard(text: 'pasted text');
      final owner = TerminalRuntimeLaunchInputOwner(
        shellLaunchesBuilder: () => const <GhosttyTerminalShellLaunch>[],
        clipboard: clipboard,
        onOsc52Blocked: () {},
      );

      String? pasted;
      await owner.pasteClipboard(
        isDisposed: false,
        onPasteText: (text) => pasted = text,
      );

      expect(pasted, equals('pasted text'));
    });

    test('pasteClipboard pastes image path when text is empty', () async {
      final clipboard = _FakeTerminalClipboard(imagePath: '/tmp/img.png');
      final owner = TerminalRuntimeLaunchInputOwner(
        shellLaunchesBuilder: () => const <GhosttyTerminalShellLaunch>[],
        clipboard: clipboard,
        onOsc52Blocked: () {},
      );

      String? pasted;
      await owner.pasteClipboard(
        isDisposed: false,
        onPasteText: (text) => pasted = text,
      );

      expect(pasted, equals('/tmp/img.png'));
    });
  });
}
