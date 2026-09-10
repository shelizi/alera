import 'package:alera/src/features/workbench/application/workbench_workspace_selection_hydrator.dart';
import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('hydrates initial terminal, tabs, and layout in order', () async {
    final events = <String>[];
    final tabs = <WorkspaceTabRecord>[_tab('tab-1')];
    final layout = WorkbenchLayout.single(
      workspaceId: 'workspace',
      tabIds: <String>['tab-1'],
    );
    final hydrator = WorkbenchWorkspaceSelectionHydrator(
      tabStore: _FakeSelectionTabStore(events: events, tabs: tabs),
      layoutResolver: _FakeSelectionLayoutResolver(
        events: events,
        layout: layout,
      ),
    );

    final result = await hydrator.hydrate(
      workspaceId: 'workspace',
      ensureInitialTerminal: true,
    );

    expect(events, <String>[
      'ensure:workspace',
      'list:workspace',
      'resolve:workspace',
    ]);
    expect(result.tabs, same(tabs));
    expect(result.layout, same(layout));
  });

  test('prompt completion hydration can skip the initial terminal', () async {
    final events = <String>[];
    final tabs = <WorkspaceTabRecord>[_tab('agent-tab')];
    final hydrator = WorkbenchWorkspaceSelectionHydrator(
      tabStore: _FakeSelectionTabStore(events: events, tabs: tabs),
      layoutResolver: _FakeSelectionLayoutResolver(
        events: events,
        layout: WorkbenchLayout.single(
          workspaceId: 'workspace',
          tabIds: <String>['agent-tab'],
        ),
      ),
    );

    await hydrator.hydrate(
      workspaceId: 'workspace',
      ensureInitialTerminal: false,
    );

    expect(events, <String>['list:workspace', 'resolve:workspace']);
  });

  test('initial terminal failure stops before listing tabs', () async {
    final events = <String>[];
    final hydrator = WorkbenchWorkspaceSelectionHydrator(
      tabStore: _FakeSelectionTabStore(
        events: events,
        tabs: const <WorkspaceTabRecord>[],
        ensureError: StateError('ensure failed'),
      ),
      layoutResolver: _FakeSelectionLayoutResolver(
        events: events,
        layout: WorkbenchLayout.single(
          workspaceId: 'workspace',
          tabIds: const <String>[],
        ),
      ),
    );

    await expectLater(
      hydrator.hydrate(workspaceId: 'workspace', ensureInitialTerminal: true),
      throwsStateError,
    );

    expect(events, <String>['ensure:workspace']);
  });

  test('tab listing failure stops before layout resolution', () async {
    final events = <String>[];
    final hydrator = WorkbenchWorkspaceSelectionHydrator(
      tabStore: _FakeSelectionTabStore(
        events: events,
        tabs: const <WorkspaceTabRecord>[],
        listError: StateError('list failed'),
      ),
      layoutResolver: _FakeSelectionLayoutResolver(
        events: events,
        layout: WorkbenchLayout.single(
          workspaceId: 'workspace',
          tabIds: const <String>[],
        ),
      ),
    );

    await expectLater(
      hydrator.hydrate(workspaceId: 'workspace', ensureInitialTerminal: false),
      throwsStateError,
    );

    expect(events, <String>['list:workspace']);
  });

  test('layout resolution failure propagates after tab listing', () async {
    final events = <String>[];
    final tabs = <WorkspaceTabRecord>[_tab('tab-1')];
    final hydrator = WorkbenchWorkspaceSelectionHydrator(
      tabStore: _FakeSelectionTabStore(events: events, tabs: tabs),
      layoutResolver: _FakeSelectionLayoutResolver(
        events: events,
        layout: WorkbenchLayout.single(
          workspaceId: 'workspace',
          tabIds: const <String>[],
        ),
        error: StateError('layout failed'),
      ),
    );

    await expectLater(
      hydrator.hydrate(workspaceId: 'workspace', ensureInitialTerminal: false),
      throwsStateError,
    );

    expect(events, <String>['list:workspace', 'resolve:workspace']);
  });
}

final class _FakeSelectionTabStore
    implements WorkbenchWorkspaceSelectionTabStore {
  _FakeSelectionTabStore({
    required this.events,
    required this.tabs,
    this.ensureError,
    this.listError,
  });

  final List<String> events;
  final List<WorkspaceTabRecord> tabs;
  final Object? ensureError;
  final Object? listError;

  @override
  Future<WorkspaceTabRecord> ensureInitialTerminalTab(
    String workspaceId,
  ) async {
    events.add('ensure:$workspaceId');
    if (ensureError case final error?) throw error;
    return tabs.isEmpty ? _tab('created-tab') : tabs.first;
  }

  @override
  Future<List<WorkspaceTabRecord>> listTabs(String workspaceId) async {
    events.add('list:$workspaceId');
    if (listError case final error?) throw error;
    return tabs;
  }
}

final class _FakeSelectionLayoutResolver
    implements WorkbenchWorkspaceSelectionLayoutResolver {
  _FakeSelectionLayoutResolver({
    required this.events,
    required this.layout,
    this.error,
  });

  final List<String> events;
  final WorkbenchLayout layout;
  final Object? error;

  @override
  Future<WorkbenchLayout> resolve({
    required String workspaceId,
    required List<WorkspaceTabRecord> tabs,
  }) async {
    events.add('resolve:$workspaceId');
    if (error case final error?) throw error;
    return layout;
  }
}

WorkspaceTabRecord _tab(String id) {
  final now = DateTime.utc(2026, 9, 11);
  return WorkspaceTabRecord(
    id: id,
    workspaceId: 'workspace',
    title: id,
    createdAt: now,
    updatedAt: now,
  );
}
