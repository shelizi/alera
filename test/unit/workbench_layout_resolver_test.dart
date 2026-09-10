import 'package:alera/src/features/workbench/application/workbench_layout_repository.dart';
import 'package:alera/src/features/workbench/application/workbench_layout_resolver.dart';
import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('creates and persists a default layout when none is stored', () async {
    final repository = _FakeLayoutRepository();
    final resolver = WorkbenchLayoutResolver(repository);
    final tabs = <WorkspaceTabRecord>[_tab('tab-a'), _tab('tab-b')];

    final layout = await resolver.resolve(workspaceId: 'workspace', tabs: tabs);

    expect(layout.workspaceId, 'workspace');
    expect(layout.activeTabId, 'tab-b');
    expect(layout.activeGroup?.tabIds, <String>['tab-a', 'tab-b']);
    expect(repository.upserts, <WorkbenchLayout>[layout]);
  });

  test('returns an unchanged stored layout without rewriting it', () async {
    final stored = WorkbenchLayout.single(
      workspaceId: 'workspace',
      tabIds: const <String>['tab-a'],
    );
    final repository = _FakeLayoutRepository(stored: stored);
    final resolver = WorkbenchLayoutResolver(repository);

    final layout = await resolver.resolve(
      workspaceId: 'workspace',
      tabs: <WorkspaceTabRecord>[_tab('tab-a')],
    );

    expect(layout, stored);
    expect(repository.upserts, isEmpty);
  });

  test('sanitizes and persists a stale stored layout', () async {
    final stored = WorkbenchLayout.single(
      workspaceId: 'workspace',
      tabIds: const <String>['stale-tab'],
    );
    final repository = _FakeLayoutRepository(stored: stored);
    final resolver = WorkbenchLayoutResolver(repository);

    final layout = await resolver.resolve(
      workspaceId: 'workspace',
      tabs: <WorkspaceTabRecord>[_tab('live-tab')],
    );

    expect(layout.activeTabId, 'live-tab');
    expect(layout.activeGroup?.tabIds, <String>['live-tab']);
    expect(repository.upserts, <WorkbenchLayout>[layout]);
    expect(layout, isNot(stored));
  });

  test('propagates persistence failures to the caller boundary', () async {
    final repository = _FakeLayoutRepository()
      ..upsertError = StateError('layout store unavailable');
    final resolver = WorkbenchLayoutResolver(repository);

    await expectLater(
      resolver.resolve(
        workspaceId: 'workspace',
        tabs: <WorkspaceTabRecord>[_tab('tab-a')],
      ),
      throwsStateError,
    );
  });
}

WorkspaceTabRecord _tab(String id) {
  final now = DateTime.utc(2026, 9, 10);
  return WorkspaceTabRecord(
    id: id,
    workspaceId: 'workspace',
    title: id,
    createdAt: now,
    updatedAt: now,
  );
}

final class _FakeLayoutRepository implements WorkbenchLayoutRepository {
  _FakeLayoutRepository({this.stored});

  WorkbenchLayout? stored;
  Object? upsertError;
  final List<WorkbenchLayout> upserts = <WorkbenchLayout>[];

  @override
  Future<WorkbenchLayout?> findWorkbenchLayout(String workspaceId) async {
    return stored;
  }

  @override
  Future<WorkbenchLayout> upsertWorkbenchLayout(WorkbenchLayout layout) async {
    if (upsertError case final Object error) {
      throw error;
    }
    stored = layout;
    upserts.add(layout);
    return layout;
  }
}
