import 'dart:async';
import 'dart:io' show Platform;

import 'package:alera/src/features/workbench/application/detached_tab_window_service.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/features/workbench/presentation/shared_engine_detached_tab_window.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:multiview_desktop/multiview_desktop.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  final now = DateTime.utc(2026, 10, 22);

  DetachedTabDescriptor fixture({
    WorkspaceTabKind kind = WorkspaceTabKind.editor,
  }) => DetachedTabDescriptor(
    workspace: Workspace(
      id: 'ws',
      projectId: 'project',
      name: 'work',
      path: r'C:\work\proj',
      createdAt: now,
      updatedAt: now,
      kind: WorkspaceKind.main,
      status: WorkspaceStatus.active,
    ),
    tab: WorkspaceTabRecord(
      id: 'tab-a',
      workspaceId: 'ws',
      title: 'file.dart',
      kind: kind,
      payload: const {'filePath': 'file.dart'},
      createdAt: now,
      updatedAt: now,
    ),
  );

  test('Terminal sharing is opt-in and other kinds retain legacy windows', () {
    final regular = SharedEngineDetachedTabWindows();
    final experimental = SharedEngineDetachedTabWindows(
      experimentalTerminalTabs: true,
    );
    addTearDown(regular.dispose);
    addTearDown(experimental.dispose);

    expect(regular.supports(fixture(kind: WorkspaceTabKind.terminal)), isFalse);
    expect(
      experimental.supports(fixture(kind: WorkspaceTabKind.terminal)),
      Platform.isWindows,
    );
    expect(
      regular.supports(fixture(kind: WorkspaceTabKind.editor)),
      Platform.isWindows,
    );
    expect(
      experimental.supports(fixture(kind: WorkspaceTabKind.editor)),
      Platform.isWindows,
    );
    for (final kind in WorkspaceTabKind.values) {
      if (kind == WorkspaceTabKind.editor ||
          kind == WorkspaceTabKind.terminal) {
        continue;
      }
      expect(experimental.supports(fixture(kind: kind)), isFalse);
    }
  });

  test('opt-in Terminal never mounts two views during a handoff', () async {
    if (!Platform.isWindows) return;
    final sourceUnmounted = Completer<void>();
    final closed = <int>[];
    final bridge = SharedEngineDetachedTabWindows(
      experimentalTerminalTabs: true,
      openNativeWindow: (_, _) async => 71,
      sourceFrameBarrier: () => sourceUnmounted.future,
      closeNativeWindow: (viewId) async {
        closed.add(viewId);
        return true;
      },
    );
    addTearDown(bridge.dispose);
    final terminal = fixture(kind: WorkspaceTabKind.terminal);
    final launched = bridge.detach(terminal);
    await Future<void>.delayed(Duration.zero);
    expect(bridge.isDetached(terminal.tab.id), isTrue);
    expect(bridge.isLaunching(terminal.tab.id), isTrue);
    expect(bridge.canRender(terminal.tab.id, 71), isFalse);
    expect(await bridge.detach(terminal), isFalse);

    sourceUnmounted.complete();
    await Future<void>.delayed(Duration.zero);
    expect(bridge.canRender(terminal.tab.id, 71), isTrue);
    bridge.reportContentMounted(terminal.tab.id, 71);
    expect(await launched, isTrue);
    expect(bridge.isLaunching(terminal.tab.id), isFalse);

    bridge.onWindowClosed(71);
    await Future<void>.delayed(Duration.zero);
    expect(bridge.isDetached(terminal.tab.id), isFalse);
    expect(closed, isEmpty);
  });

  test('Terminal close during source unmount restores the original', () async {
    if (!Platform.isWindows) return;
    final unmounted = Completer<void>();
    final bridge = SharedEngineDetachedTabWindows(
      experimentalTerminalTabs: true,
      openNativeWindow: (_, _) async => 72,
      sourceFrameBarrier: () => unmounted.future,
      closeNativeWindow: (_) async => true,
    );
    addTearDown(bridge.dispose);
    final terminal = fixture(kind: WorkspaceTabKind.terminal);
    final launched = bridge.detach(terminal);
    await Future<void>.delayed(Duration.zero);
    bridge.onWindowClosed(72);
    unmounted.complete();
    expect(await launched, isFalse);
    await Future<void>.delayed(Duration.zero);
    expect(bridge.isDetached(terminal.tab.id), isFalse);
    expect(bridge.isLaunching(terminal.tab.id), isFalse);
  });

  test(
    'two-stage handoff cannot mount child before source frame barrier',
    () async {
      final barrier = Completer<void>();
      final opened = <int>[];
      final closed = <int>[];
      final bridge = SharedEngineDetachedTabWindows(
        openNativeWindow: (builder, options) async {
          opened.add(2);
          return 2;
        },
        sourceFrameBarrier: () => barrier.future,
        closeNativeWindow: (id) async {
          closed.add(id);
          return true;
        },
      );
      addTearDown(bridge.dispose);
      final states = <String>[];
      bridge.addListener(() {
        states.add(
          '${bridge.isLaunching('tab-a')}:'
          '${bridge.isDetached('tab-a')}:'
          '${bridge.canRender('tab-a', 2)}',
        );
      });
      final launched = bridge.detach(fixture());
      await Future<void>.delayed(Duration.zero);
      expect(opened, <int>[2]);
      expect(bridge.isDetached('tab-a'), isTrue);
      expect(bridge.canRender('tab-a', 2), isFalse);
      expect(bridge.isLaunching('tab-a'), isTrue);
      barrier.complete();
      await Future<void>.delayed(Duration.zero);
      expect(bridge.canRender('tab-a', 2), isTrue);
      bridge.reportContentMounted('tab-a', 2);
      expect(await launched, isTrue);
      expect(bridge.isLaunching('tab-a'), isFalse);
      expect(bridge.detachedTabIds, contains('tab-a'));
      expect(states, contains('true:true:false'));
      expect(states, contains('true:true:true'));
      bridge.onWindowClosed(2);
      await Future<void>.delayed(Duration.zero);
      expect(bridge.isDetached('tab-a'), isFalse);
      expect(closed, isEmpty);
    },
  );

  test(
    'detached window stays hidden until mounted content owns input',
    () async {
      final shown = <int>[];
      var sawShowOnInitFalse = false;
      final bridge = SharedEngineDetachedTabWindows(
        openNativeWindow: (builder, options) async {
          sawShowOnInitFalse = options.showOnInit == false;
          return 60;
        },
        completeNativeShow: shown.add,
        sourceFrameBarrier: () => Future<void>.value(),
        closeNativeWindow: (_) async => true,
      );
      addTearDown(bridge.dispose);
      final launched = bridge.detach(fixture());
      await Future<void>.delayed(Duration.zero);
      expect(sawShowOnInitFalse, isTrue);
      // The window is created hidden; the show waiter is released only once
      // the child's input client had a chance to attach during the mount.
      expect(shown, isEmpty);
      bridge.reportContentMounted('tab-a', 60);
      expect(await launched, isTrue);
      expect(shown, <int>[60]);
    },
  );

  test('close before mount releases the pending show waiter first', () async {
    final shown = <int>[];
    final closed = <int>[];
    final bridge = SharedEngineDetachedTabWindows(
      openNativeWindow: (builder, options) async => 61,
      completeNativeShow: shown.add,
      sourceFrameBarrier: () => Future<void>.value(),
      closeNativeWindow: (id) async {
        closed.add(id);
        return true;
      },
    );
    addTearDown(bridge.dispose);
    final launched = bridge.detach(fixture());
    await Future<void>.delayed(Duration.zero);
    expect(shown, isEmpty);
    await bridge.closeTabWindow('tab-a', 61);
    // The pending openWindow future is released before the view is torn
    // down, otherwise its showOnInit wait would leak.
    expect(shown, <int>[61]);
    expect(closed, <int>[61]);
    bridge.onWindowClosed(61);
    expect(await launched, isFalse);
    await Future<void>.delayed(Duration.zero);
    expect(bridge.isDetached('tab-a'), isFalse);
  });

  test(
    'native close during unmount prevents duplicate editor and restores',
    () async {
      final barrier = Completer<void>();
      final bridge = SharedEngineDetachedTabWindows(
        openNativeWindow: (builder, options) async => 3,
        sourceFrameBarrier: () => barrier.future,
        closeNativeWindow: (_) async => true,
      );
      addTearDown(bridge.dispose);
      final result = bridge.detach(fixture());
      await Future<void>.delayed(Duration.zero);
      expect(bridge.canRender('tab-a', 3), isFalse);
      bridge.onWindowClosed(3);
      barrier.complete();
      expect(await result, isFalse);
      await Future<void>.delayed(Duration.zero);
      expect(bridge.isDetached('tab-a'), isFalse);
      expect(bridge.isLaunching('tab-a'), isFalse);
    },
  );

  test(
    'native close holds main editor hidden through child teardown frame',
    () async {
      final closeFrame = Completer<void>();
      var barrierCalls = 0;
      final bridge = SharedEngineDetachedTabWindows(
        openNativeWindow: (builder, options) async => 4,
        sourceFrameBarrier: () {
          barrierCalls += 1;
          return barrierCalls == 1 ? Future<void>.value() : closeFrame.future;
        },
        closeNativeWindow: (_) async => true,
      );
      addTearDown(bridge.dispose);
      final open = bridge.detach(fixture());
      await Future<void>.delayed(Duration.zero);
      expect(bridge.canRender('tab-a', 4), isTrue);
      bridge.reportContentMounted('tab-a', 4);
      expect(await open, isTrue);

      bridge.onWindowClosed(4);
      expect(bridge.isDetached('tab-a'), isTrue);
      expect(bridge.canRender('tab-a', 4), isFalse);
      expect(barrierCalls, 2);
      await Future<void>.delayed(Duration.zero);
      expect(bridge.isDetached('tab-a'), isTrue);
      closeFrame.complete();
      await Future<void>.delayed(Duration.zero);
      expect(bridge.isDetached('tab-a'), isFalse);
    },
  );

  test(
    'dragging a shared Editor off its child only docks on the main HWND',
    () async {
      var overMain = false;
      final closed = <int>[];
      final bridge = SharedEngineDetachedTabWindows(
        openNativeWindow: (_, _) async => 12,
        sourceFrameBarrier: () async {},
        closeNativeWindow: (id) async {
          closed.add(id);
          return true;
        },
        isCursorOverMainWindow: () async => overMain,
      );
      addTearDown(bridge.dispose);
      final launched = bridge.detach(fixture());
      await Future<void>.delayed(Duration.zero);
      bridge.reportContentMounted('tab-a', 12);
      expect(await launched, isTrue);

      expect(await bridge.tryReturnTabAtPointer('tab-a', 12), isFalse);
      expect(closed, isEmpty);
      expect(bridge.isDetached('tab-a'), isTrue);
      expect(await bridge.tryReturnTabAtPointer('tab-a', 999), isFalse);

      overMain = true;
      expect(await bridge.tryReturnTabAtPointer('tab-a', 12), isTrue);
      expect(closed, [12]);
      bridge.onWindowClosed(12);
      await Future<void>.delayed(Duration.zero);
      expect(bridge.isDetached('tab-a'), isFalse);
    },
  );

  test(
    'stale native drag hit-test cannot close an already recycled Editor window',
    () async {
      final probe = Completer<bool>();
      final closed = <int>[];
      final bridge = SharedEngineDetachedTabWindows(
        openNativeWindow: (_, _) async => 19,
        sourceFrameBarrier: () async {},
        closeNativeWindow: (id) async {
          closed.add(id);
          return true;
        },
        isCursorOverMainWindow: () => probe.future,
      );
      addTearDown(bridge.dispose);
      final launched = bridge.detach(fixture());
      await Future<void>.delayed(Duration.zero);
      bridge.reportContentMounted('tab-a', 19);
      expect(await launched, isTrue);

      final docking = bridge.tryReturnTabAtPointer('tab-a', 19);
      bridge.onWindowClosed(19);
      await Future<void>.delayed(Duration.zero);
      probe.complete(true);
      expect(await docking, isFalse);
      expect(closed, isEmpty);
      expect(bridge.isDetached('tab-a'), isFalse);
    },
  );

  test(
    'duplicate drag end requests cannot close one Editor view twice',
    () async {
      final probe = Completer<bool>();
      final closed = <int>[];
      final bridge = SharedEngineDetachedTabWindows(
        openNativeWindow: (_, _) async => 27,
        sourceFrameBarrier: () async {},
        closeNativeWindow: (id) async {
          closed.add(id);
          return true;
        },
        isCursorOverMainWindow: () => probe.future,
      );
      addTearDown(bridge.dispose);
      final launched = bridge.detach(fixture());
      await Future<void>.delayed(Duration.zero);
      bridge.reportContentMounted('tab-a', 27);
      expect(await launched, isTrue);

      final first = bridge.tryReturnTabAtPointer('tab-a', 27);
      expect(await bridge.tryReturnTabAtPointer('tab-a', 27), isFalse);
      probe.complete(true);
      expect(await first, isTrue);
      expect(closed, [27]);
    },
  );

  test(
    'closing one of two Editor windows does not restore the other',
    () async {
      var nextViewId = 40;
      final bridge = SharedEngineDetachedTabWindows(
        openNativeWindow: (_, _) async => nextViewId++,
        sourceFrameBarrier: () async {},
        closeNativeWindow: (_) async => true,
      );
      addTearDown(bridge.dispose);
      final first = fixture();
      final second = DetachedTabDescriptor(
        workspace: first.workspace,
        tab: first.tab.copyWith(
          id: 'tab-b',
          title: 'other.dart',
          payload: const {'filePath': 'other.dart'},
        ),
      );
      final a = bridge.detach(first);
      final b = bridge.detach(second);
      await Future<void>.delayed(Duration.zero);
      bridge.reportContentMounted('tab-a', 40);
      bridge.reportContentMounted('tab-b', 41);
      expect(await a, isTrue);
      expect(await b, isTrue);
      expect(bridge.detachedTabIds, {'tab-a', 'tab-b'});

      bridge.onWindowClosed(40);
      await Future<void>.delayed(Duration.zero);
      expect(bridge.isDetached('tab-a'), isFalse);
      expect(bridge.isDetached('tab-b'), isTrue);
      expect(bridge.canRender('tab-b', 41), isTrue);
      bridge.onWindowClosed(41);
      await Future<void>.delayed(Duration.zero);
      expect(bridge.detachedTabIds, isEmpty);
    },
  );

  test('a reopened child keeps the last physical frame', () async {
    if (!Platform.isWindows) return;
    final opened = <WindowOptions>[];
    final written = <(int, Rect)>[];
    var nextViewId = 80;
    Rect? live;
    final bridge = SharedEngineDetachedTabWindows(
      experimentalTerminalTabs: true,
      openNativeWindow: (_, options) async {
        opened.add(options);
        return nextViewId++;
      },
      sourceFrameBarrier: () async {},
      closeNativeWindow: (_) async => true,
      readPhysicalBounds: (_) => live,
      writePhysicalBounds: (viewId, bounds) {
        written.add((viewId, bounds));
        return true;
      },
    );
    addTearDown(bridge.dispose);
    final terminal = fixture(kind: WorkspaceTabKind.terminal);

    final first = bridge.detach(terminal);
    await Future<void>.delayed(Duration.zero);
    bridge.reportContentMounted(terminal.tab.id, 80);
    expect(await first, isTrue);
    expect(opened.single.alignment, Alignment.center);
    expect(opened.single.size, const Size(1050, 720));
    expect(written, isEmpty);

    live = const Rect.fromLTWH(-1700, 40, 1500, 900);
    bridge.onWindowEvent(80, 'moved');
    bridge.onWindowEvent(80, 'close');
    bridge.onWindowClosed(80);
    await Future<void>.delayed(Duration.zero);
    expect(bridge.isDetached(terminal.tab.id), isFalse);

    final second = bridge.detach(terminal);
    await Future<void>.delayed(Duration.zero);
    bridge.reportContentMounted(terminal.tab.id, 81);
    expect(await second, isTrue);
    expect(opened, hasLength(2));
    expect(opened[1].alignment, isNull);
    expect(written, <(int, Rect)>[(81, live!)]);
  });

  test('a move event remembers a drag that never emits moved', () async {
    if (!Platform.isWindows) return;
    final opened = <WindowOptions>[];
    final written = <(int, Rect)>[];
    var nextViewId = 90;
    Rect? live;
    final bridge = SharedEngineDetachedTabWindows(
      experimentalTerminalTabs: true,
      physicalBoundsPollInterval: Duration.zero,
      openNativeWindow: (_, options) async {
        opened.add(options);
        return nextViewId++;
      },
      sourceFrameBarrier: () async {},
      closeNativeWindow: (_) async => true,
      readPhysicalBounds: (_) => live,
      writePhysicalBounds: (viewId, bounds) {
        written.add((viewId, bounds));
        return true;
      },
    );
    addTearDown(bridge.dispose);
    final terminal = fixture(kind: WorkspaceTabKind.terminal);
    const dragged = Rect.fromLTWH(120, 100, 1600, 1000);

    final first = bridge.detach(terminal);
    await Future<void>.delayed(Duration.zero);
    bridge.reportContentMounted(terminal.tab.id, 90);
    expect(await first, isTrue);

    live = dragged;
    bridge.onWindowEvent(90, 'move');
    live = null;
    bridge.onWindowClosed(90);
    await Future<void>.delayed(Duration.zero);

    final second = bridge.detach(terminal);
    await Future<void>.delayed(Duration.zero);
    bridge.reportContentMounted(terminal.tab.id, 91);
    expect(await second, isTrue);
    expect(opened[1].alignment, isNull);
    expect(written, <(int, Rect)>[(91, dragged)]);
  });

  test('a bounds poll remembers a drag that never emits an event', () async {
    if (!Platform.isWindows) return;
    final opened = <WindowOptions>[];
    final written = <(int, Rect)>[];
    var nextViewId = 100;
    Rect? live;
    final bridge = SharedEngineDetachedTabWindows(
      experimentalTerminalTabs: true,
      physicalBoundsPollInterval: const Duration(milliseconds: 20),
      openNativeWindow: (_, options) async {
        opened.add(options);
        return nextViewId++;
      },
      sourceFrameBarrier: () async {},
      closeNativeWindow: (_) async => true,
      readPhysicalBounds: (_) => live,
      writePhysicalBounds: (viewId, bounds) {
        written.add((viewId, bounds));
        return true;
      },
    );
    addTearDown(bridge.dispose);
    final terminal = fixture(kind: WorkspaceTabKind.terminal);
    const dragged = Rect.fromLTWH(1900, 80, 1400, 900);

    final first = bridge.detach(terminal);
    await Future<void>.delayed(Duration.zero);
    bridge.reportContentMounted(terminal.tab.id, 100);
    expect(await first, isTrue);
    expect(opened.single.alignment, Alignment.center);
    expect(written, isEmpty);

    live = dragged;
    await Future<void>.delayed(const Duration(milliseconds: 150));
    live = null;
    bridge.onWindowClosed(100);
    await Future<void>.delayed(Duration.zero);
    expect(bridge.isDetached(terminal.tab.id), isFalse);

    final second = bridge.detach(terminal);
    await Future<void>.delayed(Duration.zero);
    bridge.reportContentMounted(terminal.tab.id, 101);
    expect(await second, isTrue);
    expect(opened[1].alignment, isNull);
    expect(written, <(int, Rect)>[(101, dragged)]);
  });
}
