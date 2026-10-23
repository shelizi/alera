import 'package:alera_multiview_poc/tab_lease_registry.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  late TabLeaseRegistry registry;
  late DemoTabSession editor;

  setUp(() {
    registry = TabLeaseRegistry(mainViewId: 1);
    editor = DemoTabSession(
      tabId: 'editor/test.dart',
      kind: TabContentKind.editor,
      initialText: 'saved file',
    );
    registry.register(editor);
  });

  tearDown(() => registry.dispose());

  test('source must unmount before target may commit', () {
    registry.claimMount(editor.tabId, 1);
    editor.editorText.text = 'unsaved body';
    final ticket = registry.beginTransfer(editor.tabId, 2);
    expect(registry.ownerOf(editor.tabId), isNull);
    expect(registry.canMount(editor.tabId, 1), isFalse);
    expect(registry.canMount(editor.tabId, 2), isFalse);
    expect(() => registry.commitTransfer(ticket), throwsStateError);
    registry.releaseMount(editor.tabId, 1);
    expect(registry.commitTransfer(ticket), isTrue);
    expect(registry.ownerOf(editor.tabId), 2);
    registry.claimMount(editor.tabId, 2);
    expect(() => registry.claimMount(editor.tabId, 1), throwsStateError);
    expect(() => registry.claimMount(editor.tabId, 2), throwsStateError);
    expect(editor.editorText.text, 'unsaved body');
    expect(editor.isDirty, isTrue);
  });

  test('100 atomic moves preserve editor buffer and selection', () {
    editor.editorText.value = const TextEditingValue(
      text: 'abcdef-UNSAVED',
      selection: TextSelection.collapsed(offset: 7),
    );
    for (var i = 0; i < 100; i++) {
      final from = i.isEven ? 1 : 2;
      final to = i.isEven ? 2 : 1;
      registry.claimMount(editor.tabId, from);
      final ticket = registry.beginTransfer(editor.tabId, to);
      expect(registry.ownerOf(editor.tabId), isNull);
      registry.releaseMount(editor.tabId, from);
      expect(registry.commitTransfer(ticket), isTrue);
      expect(editor.editorText.text, 'abcdef-UNSAVED');
      expect(editor.editorText.selection.baseOffset, 7);
      expect(editor.isDirty, isTrue);
    }
    expect(registry.ownerOf(editor.tabId), 1);
    editor.saveSnapshot();
    expect(editor.isDirty, isFalse);
  });

  test('failed transfer rolls back; stale ticket cannot commit', () {
    final failed = registry.beginTransfer(editor.tabId, 2);
    expect(registry.rollbackTransfer(failed), isTrue);
    expect(registry.ownerOf(editor.tabId), 1);
    expect(registry.commitTransfer(failed), isFalse);
    final latest = registry.beginTransfer(editor.tabId, 3);
    registry.recoverClosedWindow(3);
    expect(registry.commitTransfer(latest), isFalse);
    expect(registry.ownerOf(editor.tabId), 1);
  });

  test('window close returns dirty owner to main and keeps terminal host', () {
    final terminal = DemoTabSession(
      tabId: 'terminal/run',
      kind: TabContentKind.terminal,
      initialText: '',
    );
    registry.register(terminal);
    terminal.terminalHostPid = 42;
    terminal.terminalHostStartCount = 1;
    terminal.appendTerminalOutput('hello before detach');
    final ticket = registry.beginTransfer(terminal.tabId, 2);
    expect(registry.commitTransfer(ticket), isTrue);
    terminal.appendTerminalOutput('hello while detached');
    registry.recoverClosedWindow(2);
    expect(registry.ownerOf(terminal.tabId), 1);
    expect(terminal.terminalHostPid, 42);
    expect(terminal.terminalHostStartCount, 1);
    expect(terminal.terminalTranscript, hasLength(2));
  });

  testWidgets('source Widget unmounts before the target view mounts', (
    tester,
  ) async {
    Widget screenFor(int viewId) => MaterialApp(
      home: AnimatedBuilder(
        animation: registry,
        builder: (_, _) => Scaffold(
          body: registry.canMount(editor.tabId, viewId)
              ? EditorLeaseView(
                  registry: registry,
                  tabId: editor.tabId,
                  viewId: viewId,
                )
              : const Text('Not the owner'),
        ),
      ),
    );

    await tester.pumpWidget(screenFor(1));
    expect(find.byType(TextField), findsOneWidget);
    expect(registry.mountedIn(editor.tabId), 1);

    final ticket = registry.beginTransfer(editor.tabId, 2);
    await tester.pump();
    expect(find.byType(TextField), findsNothing);
    expect(registry.mountedIn(editor.tabId), isNull);
    expect(registry.commitTransfer(ticket), isTrue);

    await tester.pumpWidget(screenFor(2));
    expect(find.byType(TextField), findsOneWidget);
    expect(registry.mountedIn(editor.tabId), 2);
    await tester.pumpWidget(const SizedBox.shrink());
    expect(registry.mountedIn(editor.tabId), isNull);
  });
}
