import 'dart:async';

import 'package:alera/src/features/workbench/application/workbench_layout_repository.dart';
import 'package:alera/src/features/workbench/application/workbench_sequenced_layout_repository.dart';
import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('serializes writes for the same workspace', () async {
    final delegate = _GateLayoutRepository(blockWorkspaceId: 'workspace');
    final repository = WorkbenchSequencedLayoutRepository(delegate);
    final firstLayout = _layout('workspace', 'first');
    final secondLayout = _layout('workspace', 'second');

    final first = repository.upsertWorkbenchLayout(firstLayout);
    await delegate.blocked.future;
    final second = repository.upsertWorkbenchLayout(secondLayout);
    await Future<void>.delayed(Duration.zero);

    expect(delegate.started, <WorkbenchLayout>[firstLayout]);

    delegate.release.complete();
    await Future.wait(<Future<WorkbenchLayout>>[first, second]);

    expect(delegate.started, <WorkbenchLayout>[firstLayout, secondLayout]);
    expect(delegate.completed, <WorkbenchLayout>[firstLayout, secondLayout]);
  });

  test('does not serialize writes for different workspaces', () async {
    final delegate = _GateLayoutRepository(blockWorkspaceId: 'workspace-a');
    final repository = WorkbenchSequencedLayoutRepository(delegate);
    final blockedLayout = _layout('workspace-a', 'blocked');
    final otherLayout = _layout('workspace-b', 'other');

    final blocked = repository.upsertWorkbenchLayout(blockedLayout);
    await delegate.blocked.future;
    await repository.upsertWorkbenchLayout(otherLayout);

    expect(delegate.started, <WorkbenchLayout>[blockedLayout, otherLayout]);
    expect(delegate.completed, <WorkbenchLayout>[otherLayout]);

    delegate.release.complete();
    await blocked;
  });

  test('a failed write does not block the next write', () async {
    final delegate = _GateLayoutRepository()..failNext = true;
    final repository = WorkbenchSequencedLayoutRepository(delegate);
    final failedLayout = _layout('workspace', 'failed');
    final nextLayout = _layout('workspace', 'next');

    await expectLater(
      repository.upsertWorkbenchLayout(failedLayout),
      throwsStateError,
    );
    await repository.upsertWorkbenchLayout(nextLayout);

    expect(delegate.completed, <WorkbenchLayout>[nextLayout]);
  });
}

WorkbenchLayout _layout(String workspaceId, String tabId) =>
    WorkbenchLayout.single(workspaceId: workspaceId, tabIds: <String>[tabId]);

final class _GateLayoutRepository implements WorkbenchLayoutRepository {
  _GateLayoutRepository({this.blockWorkspaceId});

  final String? blockWorkspaceId;
  final Completer<void> blocked = Completer<void>();
  final Completer<void> release = Completer<void>();
  final List<WorkbenchLayout> started = <WorkbenchLayout>[];
  final List<WorkbenchLayout> completed = <WorkbenchLayout>[];
  bool failNext = false;
  bool _blockedOnce = false;

  @override
  Future<WorkbenchLayout?> findWorkbenchLayout(String workspaceId) async =>
      null;

  @override
  Future<WorkbenchLayout> upsertWorkbenchLayout(WorkbenchLayout layout) async {
    started.add(layout);
    if (failNext) {
      failNext = false;
      throw StateError('write failed');
    }
    if (!_blockedOnce && blockWorkspaceId == layout.workspaceId) {
      _blockedOnce = true;
      blocked.complete();
      await release.future;
    }
    completed.add(layout);
    return layout;
  }
}
