part of 'terminal_runtime_native_test.dart';

void _registerTerminalRuntimeRendererAdapterOwnerTests() {
  group('TerminalRuntimeRendererAdapterOwner', () {
    test('createTerminal applies scrollback and separator settings', () {
      final owner = TerminalRuntimeRendererAdapterOwner();
      final terminal = owner.createTerminal(
        settings: TerminalSettings.defaults.copyWith(scrollbackLines: 42),
      );
      addTearDown(terminal.dispose);

      expect(terminal.maxLines, 42);
    });

    test('attachTerminal wires callbacks and detachTerminal clears them', () {
      final owner = TerminalRuntimeRendererAdapterOwner();
      final terminal = owner.createTerminal(
        settings: TerminalSettings.defaults,
      );
      addTearDown(terminal.dispose);

      owner.attachTerminal(
        terminal,
        onTitleChange: (_) {},
        onOutput: (_) {},
        onResize: (_, _, _, _) {},
        onClipboardStore: (_) {},
      );
      expect(terminal.onTitleChange, isNotNull);
      expect(terminal.onOutput, isNotNull);
      expect(terminal.onResize, isNotNull);
      expect(terminal.onClipboardStore, isNotNull);

      owner.detachTerminal(terminal);
      expect(terminal.onTitleChange, isNull);
      expect(terminal.onOutput, isNull);
      expect(terminal.onResize, isNull);
      expect(terminal.onClipboardStore, isNull);
    });

    test('resolveFontFamily keeps JetBrains Mono and falls back otherwise', () {
      final owner = TerminalRuntimeRendererAdapterOwner();
      expect(owner.resolveFontFamily('JetBrains Mono'), 'JetBrains Mono');
      expect(owner.resolveFontFamily(' jetbrains mono '), 'JetBrains Mono');
      expect(owner.resolveFontFamily(''), 'monospace');
      expect(owner.resolveFontFamily('Fira Code'), 'Fira Code');
    });

    test('resolveCursorType maps cursor shapes to xterm cursor types', () {
      final owner = TerminalRuntimeRendererAdapterOwner();
      expect(
        owner.resolveCursorType(TerminalCursorShape.block),
        xterm.TerminalCursorType.block,
      );
      expect(
        owner.resolveCursorType(TerminalCursorShape.bar),
        xterm.TerminalCursorType.verticalBar,
      );
      expect(
        owner.resolveCursorType(TerminalCursorShape.underline),
        xterm.TerminalCursorType.underline,
      );
    });
  });
}
