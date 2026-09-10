import 'package:alera/src/features/workbench/application/workbench_cleared_layout_registry.dart';
import 'package:alera/src/features/workbench/application/workbench_hosted_review_retention_service.dart';
import 'package:alera/src/features/workbench/application/workbench_repository.dart';
import 'package:alera/src/features/workbench/application/workbench_sleep_workspace_coordinator.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_focus_history.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'sleep cleanup clears persisted tabs before releasing tab retentions',
    () async {
      final events = <String>[];
      final removal = _FakeTabRemoval(events: events);
      final retention = _FakeRetention(events: events);
      final clearedLayouts = WorkbenchClearedLayoutRegistry();
      final focusHistory = WorkspaceTabFocusHistory()
        ..record('workspace', 'tab-2');
      final coordinator = WorkbenchSleepWorkspaceCoordinator(
        tabRemoval: removal,
        hostedReviewRetention: retention,
        clearedLayouts: clearedLayouts,
        tabFocusHistory: focusHistory,
      );
      final tabs = <WorkspaceTabRecord>[_tab('tab-1'), _tab('tab-2')];

      await coordinator.sleep(workspace: _workspace(), tabs: tabs);

      expect(events, <String>[
        'remove:workspace',
        'release:tab-1',
        'release:tab-2',
      ]);
      expect(clearedLayouts.contains('workspace'), isTrue);
      expect(
        focusHistory.mostRecentOpen('workspace', <String>{'tab-2'}),
        isNull,
      );
    },
  );

  test(
    'persistence failure rolls back only the cleared-layout marker',
    () async {
      final events = <String>[];
      final removal = _FakeTabRemoval(
        events: events,
        error: StateError('remove failed'),
      );
      final retention = _FakeRetention(events: events);
      final clearedLayouts = WorkbenchClearedLayoutRegistry();
      final focusHistory = WorkspaceTabFocusHistory()
        ..record('workspace', 'tab-1');
      final coordinator = WorkbenchSleepWorkspaceCoordinator(
        tabRemoval: removal,
        hostedReviewRetention: retention,
        clearedLayouts: clearedLayouts,
        tabFocusHistory: focusHistory,
      );

      await expectLater(
        coordinator.sleep(
          workspace: _workspace(),
          tabs: <WorkspaceTabRecord>[_tab('tab-1')],
        ),
        throwsA(isA<StateError>()),
      );

      expect(events, <String>['remove:workspace']);
      expect(clearedLayouts.contains('workspace'), isFalse);
      expect(
        focusHistory.mostRecentOpen('workspace', <String>{'tab-1'}),
        isNull,
      );
    },
  );

  test('retention failure rolls back the cleared-layout marker', () async {
    final events = <String>[];
    final removal = _FakeTabRemoval(events: events);
    final retention = _FakeRetention(events: events, failOnTabId: 'tab-2');
    final clearedLayouts = WorkbenchClearedLayoutRegistry();
    final focusHistory = WorkspaceTabFocusHistory()
      ..record('workspace', 'tab-1');
    final coordinator = WorkbenchSleepWorkspaceCoordinator(
      tabRemoval: removal,
      hostedReviewRetention: retention,
      clearedLayouts: clearedLayouts,
      tabFocusHistory: focusHistory,
    );

    await expectLater(
      coordinator.sleep(
        workspace: _workspace(),
        tabs: <WorkspaceTabRecord>[_tab('tab-1'), _tab('tab-2')],
      ),
      throwsA(isA<StateError>()),
    );

    expect(events, <String>[
      'remove:workspace',
      'release:tab-1',
      'release:tab-2',
    ]);
    expect(clearedLayouts.contains('workspace'), isFalse);
    expect(focusHistory.mostRecentOpen('workspace', <String>{'tab-1'}), isNull);
  });
}

final class _FakeTabRemoval implements WorkbenchWorkspaceTabRemovalRepository {
  _FakeTabRemoval({required this.events, this.error});

  final List<String> events;
  final Object? error;

  @override
  Future<void> removeWorkspaceTabsForWorkspace(String workspaceId) async {
    events.add('remove:$workspaceId');
    if (error case final error?) throw error;
  }
}

final class _FakeRetention implements WorkbenchHostedReviewTabRetention {
  _FakeRetention({required this.events, this.failOnTabId});

  final List<String> events;
  final String? failOnTabId;

  @override
  Future<void> releaseTab(
    Workspace workspace,
    WorkspaceTabRecord tab, {
    String? fallbackWorkspacePath,
  }) async {
    events.add('release:${tab.id}');
    if (tab.id == failOnTabId) throw StateError('retention failed');
  }
}

Workspace _workspace() => Workspace(
  id: 'workspace',
  projectId: 'project',
  name: 'Workspace',
  path: 'workspace-path',
  createdAt: DateTime.utc(2026, 9, 11),
  updatedAt: DateTime.utc(2026, 9, 11),
  kind: WorkspaceKind.linked,
  status: WorkspaceStatus.active,
);

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
