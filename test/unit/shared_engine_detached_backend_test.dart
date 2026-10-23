import 'dart:async';

import 'package:alera/src/features/workbench/application/detached_tab_window_service.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';

class _FakeBackend extends ChangeNotifier
    implements SharedEngineDetachedTabBackend {
  final pending = <String>{};
  final active = <String>{};
  final Completer<bool> result = Completer<bool>();
  int launches = 0;

  @override
  bool supports(DetachedTabDescriptor d) =>
      d.tab.kind == WorkspaceTabKind.editor;

  @override
  Set<String> get detachedTabIds => Set.unmodifiable(active);

  @override
  bool isDetached(String id) => active.contains(id);

  @override
  bool isLaunching(String id) => pending.contains(id);

  @override
  Future<bool> detach(DetachedTabDescriptor d) async {
    final id = d.tab.id;
    if (!pending.add(id) || active.contains(id)) return false;
    launches++;
    notifyListeners();
    try {
      if (await result.future) {
        active.add(id);
        notifyListeners();
        return true;
      }
      return false;
    } finally {
      pending.remove(id);
      notifyListeners();
    }
  }

  void close(String id) {
    active.remove(id);
    notifyListeners();
  }
}

void main() {
  final now = DateTime.utc(2026, 10, 22);
  DetachedTabDescriptor fixture(WorkspaceTabKind kind) => DetachedTabDescriptor(
    workspace: Workspace(
      id: 'ws',
      projectId: 'project',
      name: 'ws',
      path: r'C:\work\test',
      createdAt: now,
      updatedAt: now,
      kind: WorkspaceKind.main,
      status: WorkspaceStatus.active,
    ),
    tab: WorkspaceTabRecord(
      id: 'editor-tab',
      workspaceId: 'ws',
      title: 'demo.dart',
      kind: kind,
      createdAt: now,
      updatedAt: now,
      payload: const {'filePath': 'demo.dart'},
    ),
  );

  test(
    'opt-in shared backend routes supported editor without new process',
    () async {
      var fallbackCalls = 0;
      final backend = _FakeBackend();
      final service = DetachedTabWindowService(
        starter: (_, _) async {
          fallbackCalls++;
          throw StateError('multi-process fallback');
        },
      );
      addTearDown(() {
        service.dispose();
        backend.dispose();
      });
      final editor = fixture(WorkspaceTabKind.editor);
      expect(service.willShareSession(editor), isFalse);
      service.useExperimentalSharedBackend(backend);
      expect(service.willShareSession(editor), isTrue);
      // Windows has a shared Engine by default, but only Editor widgets
      // have a verified per-view lifecycle. Never silently route Terminal,
      // Git Diff, Markdown, PDF or Git History through that backend.
      for (final kind in WorkspaceTabKind.values) {
        if (kind == WorkspaceTabKind.editor) continue;
        expect(
          service.willShareSession(fixture(kind)),
          isFalse,
          reason: '${kind.name} must retain the legacy child-process path',
        );
      }
      final snapshots = <String>[];
      service.addListener(() {
        snapshots.add(
          '${service.isLaunching(editor.tab.id)}:'
          '${service.isDetached(editor.tab.id)}',
        );
      });
      final launched = service.detach(editor);
      expect(service.isLaunching(editor.tab.id), isTrue);
      expect(await service.detach(editor), isFalse);
      backend.result.complete(true);
      expect(await launched, isTrue);
      expect(service.isDetached(editor.tab.id), isTrue);
      expect(service.detachedTabIds, contains(editor.tab.id));
      expect(snapshots, contains('true:false'));
      expect(snapshots, contains('false:true'));
      expect(fallbackCalls, 0);
      backend.close(editor.tab.id);
      expect(service.isDetached(editor.tab.id), isFalse);
      expect(backend.launches, 1);

      await expectLater(
        service.detach(fixture(WorkspaceTabKind.terminal)),
        throwsStateError,
      );
      expect(fallbackCalls, 1);
      service.useExperimentalSharedBackend(null);
      expect(service.willShareSession(editor), isFalse);
    },
  );

  test('shared launch failure keeps original tab and clears pending', () async {
    final backend = _FakeBackend();
    final service = DetachedTabWindowService(
      starter: (_, _) => throw StateError('unexpected fallback'),
    );
    addTearDown(() {
      service.dispose();
      backend.dispose();
    });
    service.useExperimentalSharedBackend(backend);
    final launched = service.detach(fixture(WorkspaceTabKind.editor));
    expect(service.isLaunching('editor-tab'), isTrue);
    backend.result.complete(false);
    expect(await launched, isFalse);
    expect(service.isLaunching('editor-tab'), isFalse);
    expect(service.isDetached('editor-tab'), isFalse);
  });
}
