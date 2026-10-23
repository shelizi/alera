import 'dart:async';

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

  DetachedTabDescriptor fixture() => DetachedTabDescriptor(
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
      kind: WorkspaceTabKind.editor,
      payload: const {'filePath': 'file.dart'},
      createdAt: now,
      updatedAt: now,
    ),
  );

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
}
