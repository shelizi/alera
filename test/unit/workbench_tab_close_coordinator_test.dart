import 'package:alera/src/features/workbench/application/workbench_closed_tabs_plan.dart';
import 'package:alera/src/features/workbench/application/workbench_explicit_resource_cleaner.dart';
import 'package:alera/src/features/workbench/application/workbench_hosted_review_retention_service.dart';
import 'package:alera/src/features/workbench/application/workbench_tab_close_coordinator.dart';
import 'package:alera/src/features/workbench/application/workbench_tab_close_store.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('closes each known tab in persistence-retention-local order', () async {
    final events = <String>[];
    final coordinator = WorkbenchTabCloseCoordinator(
      tabStore: _FakeCloseStore(events),
      hostedReviewRetention: _FakeTabRetention(events),
      resourceCleaner: _FakeTabCleaner(events),
    );
    final first = _tab('tab-1');
    final second = _tab('tab-2');

    await coordinator.close(
      workspace: _workspace(),
      snapshot: WorkbenchTabCloseSnapshot(
        closedTabIds: <String>{first.id, second.id},
        closingTabs: <String, WorkspaceTabRecord>{
          first.id: first,
          second.id: second,
        },
        closedActiveTab: true,
      ),
    );

    expect(events, <String>[
      'close:tab-1',
      'release:tab-1',
      'local:tab-1',
      'close:tab-2',
      'release:tab-2',
      'local:tab-2',
    ]);
  });

  test(
    'unknown requested tab skips retention but still clears local resources',
    () async {
      final events = <String>[];
      final coordinator = WorkbenchTabCloseCoordinator(
        tabStore: _FakeCloseStore(events),
        hostedReviewRetention: _FakeTabRetention(events),
        resourceCleaner: _FakeTabCleaner(events),
      );

      await coordinator.close(
        workspace: _workspace(),
        snapshot: const WorkbenchTabCloseSnapshot(
          closedTabIds: <String>{'missing-tab'},
          closingTabs: <String, WorkspaceTabRecord>{},
          closedActiveTab: false,
        ),
      );

      expect(events, <String>['close:missing-tab', 'local:missing-tab']);
    },
  );

  test(
    'persistence failure stops before retention cleanup and later tabs',
    () async {
      final events = <String>[];
      final coordinator = WorkbenchTabCloseCoordinator(
        tabStore: _FakeCloseStore(events, failOnTabId: 'tab-1'),
        hostedReviewRetention: _FakeTabRetention(events),
        resourceCleaner: _FakeTabCleaner(events),
      );
      final first = _tab('tab-1');
      final second = _tab('tab-2');

      await expectLater(
        coordinator.close(
          workspace: _workspace(),
          snapshot: WorkbenchTabCloseSnapshot(
            closedTabIds: <String>{first.id, second.id},
            closingTabs: <String, WorkspaceTabRecord>{
              first.id: first,
              second.id: second,
            },
            closedActiveTab: false,
          ),
        ),
        throwsStateError,
      );

      expect(events, <String>['close:tab-1']);
    },
  );

  test('retention failure stops before local cleanup and later tabs', () async {
    final events = <String>[];
    final coordinator = WorkbenchTabCloseCoordinator(
      tabStore: _FakeCloseStore(events),
      hostedReviewRetention: _FakeTabRetention(events, failOnTabId: 'tab-1'),
      resourceCleaner: _FakeTabCleaner(events),
    );
    final first = _tab('tab-1');
    final second = _tab('tab-2');

    await expectLater(
      coordinator.close(
        workspace: _workspace(),
        snapshot: WorkbenchTabCloseSnapshot(
          closedTabIds: <String>{first.id, second.id},
          closingTabs: <String, WorkspaceTabRecord>{
            first.id: first,
            second.id: second,
          },
          closedActiveTab: false,
        ),
      ),
      throwsStateError,
    );

    expect(events, <String>['close:tab-1', 'release:tab-1']);
  });
}

final class _FakeCloseStore implements WorkbenchTabCloseStore {
  _FakeCloseStore(this.events, {this.failOnTabId});

  final List<String> events;
  final String? failOnTabId;

  @override
  Future<void> closeTab(String tabId) async {
    events.add('close:$tabId');
    if (tabId == failOnTabId) throw StateError('close failed');
  }
}

final class _FakeTabRetention implements WorkbenchHostedReviewTabRetention {
  _FakeTabRetention(this.events, {this.failOnTabId});

  final List<String> events;
  final String? failOnTabId;

  @override
  Future<void> releaseTab(
    Workspace workspace,
    WorkspaceTabRecord tab, {
    String? fallbackWorkspacePath,
  }) async {
    events.add('release:${tab.id}');
    if (tab.id == failOnTabId) throw StateError('release failed');
  }
}

final class _FakeTabCleaner implements WorkbenchExplicitTabResourceCleaner {
  _FakeTabCleaner(this.events);

  final List<String> events;

  @override
  void closeTabLocalResources(String tabId) => events.add('local:$tabId');
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
