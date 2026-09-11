import 'dart:async';

import 'package:alera/src/features/workbench/application/workbench_layout_load_coordinator.dart';
import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('loads tabs, resolves the layout, then applies it', () async {
    final coordinator = WorkbenchLayoutLoadCoordinator();
    final events = <String>[];
    final tabs = <WorkspaceTabRecord>[_tab('tab-a')];
    final layout = WorkbenchLayout.single(
      workspaceId: 'workspace',
      tabIds: const <String>['tab-a'],
    );

    await coordinator.load(
      workspaceId: 'workspace',
      listTabs: (workspaceId) async {
        events.add('tabs-$workspaceId');
        return tabs;
      },
      resolveLayout: ({required workspaceId, required tabs}) async {
        events.add('resolve-$workspaceId-${tabs.single.id}');
        return layout;
      },
      applyLayout: (resolved) async {
        expect(resolved, layout);
        events.add('apply');
      },
      onError: (_) => events.add('error'),
    );

    expect(events, <String>[
      'tabs-workspace',
      'resolve-workspace-tab-a',
      'apply',
    ]);
  });

  test('coalesces concurrent loads for the same workspace', () async {
    final coordinator = WorkbenchLayoutLoadCoordinator();
    final gate = Completer<void>();
    var listCalls = 0;
    var resolveCalls = 0;
    var applyCalls = 0;

    Future<List<WorkspaceTabRecord>> listTabs(String _) async {
      listCalls += 1;
      await gate.future;
      return <WorkspaceTabRecord>[_tab('tab-a')];
    }

    final first = coordinator.load(
      workspaceId: 'workspace',
      listTabs: listTabs,
      resolveLayout: ({required workspaceId, required tabs}) async {
        resolveCalls += 1;
        return WorkbenchLayout.single(
          workspaceId: workspaceId,
          tabIds: <String>[for (final tab in tabs) tab.id],
        );
      },
      applyLayout: (_) async => applyCalls += 1,
      onError: (_) {},
    );
    final second = coordinator.load(
      workspaceId: 'workspace',
      listTabs: listTabs,
      resolveLayout: ({required workspaceId, required tabs}) async {
        resolveCalls += 1;
        return WorkbenchLayout.single(
          workspaceId: workspaceId,
          tabIds: <String>[for (final tab in tabs) tab.id],
        );
      },
      applyLayout: (_) async => applyCalls += 1,
      onError: (_) {},
    );

    await second;
    expect(listCalls, 1);
    expect(resolveCalls, 0);
    expect(applyCalls, 0);

    gate.complete();
    await first;
    expect(listCalls, 1);
    expect(resolveCalls, 1);
    expect(applyCalls, 1);
  });

  test('different workspaces can load concurrently', () async {
    final coordinator = WorkbenchLayoutLoadCoordinator();
    final firstGate = Completer<void>();
    final started = <String>[];

    Future<List<WorkspaceTabRecord>> listTabs(String workspaceId) async {
      started.add(workspaceId);
      if (workspaceId == 'first') {
        await firstGate.future;
      }
      return const <WorkspaceTabRecord>[];
    }

    Future<WorkbenchLayout> resolveLayout({
      required String workspaceId,
      required List<WorkspaceTabRecord> tabs,
    }) async {
      return WorkbenchLayout.single(workspaceId: workspaceId, tabIds: const []);
    }

    final first = coordinator.load(
      workspaceId: 'first',
      listTabs: listTabs,
      resolveLayout: resolveLayout,
      applyLayout: (_) async {},
      onError: (_) {},
    );
    final second = coordinator.load(
      workspaceId: 'second',
      listTabs: listTabs,
      resolveLayout: resolveLayout,
      applyLayout: (_) async {},
      onError: (_) {},
    );

    await second;
    expect(started, containsAll(<String>['first', 'second']));

    firstGate.complete();
    await first;
  });

  test('stale loads skip layout application and stale errors', () async {
    final coordinator = WorkbenchLayoutLoadCoordinator();
    final events = <String>[];
    var current = true;

    await coordinator.load(
      workspaceId: 'workspace',
      listTabs: (_) async => <WorkspaceTabRecord>[_tab('tab-a')],
      resolveLayout: ({required workspaceId, required tabs}) async {
        current = false;
        return WorkbenchLayout.single(
          workspaceId: workspaceId,
          tabIds: <String>[for (final tab in tabs) tab.id],
        );
      },
      applyLayout: (_) async => events.add('apply'),
      isLoadCurrent: () => current,
      onError: (_) => events.add('error'),
    );

    expect(events, isEmpty);

    current = true;
    await coordinator.load(
      workspaceId: 'workspace',
      listTabs: (_) async {
        current = false;
        throw StateError('stale failure');
      },
      resolveLayout: ({required workspaceId, required tabs}) async =>
          WorkbenchLayout.single(workspaceId: workspaceId, tabIds: const []),
      applyLayout: (_) async => events.add('apply'),
      isLoadCurrent: () => current,
      onError: (_) => events.add('error'),
    );

    expect(events, isEmpty);
  });

  test('reports failures, releases the gate, and permits retry', () async {
    final coordinator = WorkbenchLayoutLoadCoordinator();
    final errors = <Object>[];
    var listCalls = 0;

    Future<void> load() => coordinator.load(
      workspaceId: 'workspace',
      listTabs: (_) async {
        listCalls += 1;
        if (listCalls == 1) {
          throw StateError('layout load failed');
        }
        return const <WorkspaceTabRecord>[];
      },
      resolveLayout: ({required workspaceId, required tabs}) async =>
          WorkbenchLayout.single(workspaceId: workspaceId, tabIds: const []),
      applyLayout: (_) async {},
      onError: errors.add,
    );

    await load();
    await load();

    expect(listCalls, 2);
    expect(errors, hasLength(1));
    expect(errors.single, isA<StateError>());
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
