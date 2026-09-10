import 'package:alera/src/features/workbench/application/workbench_hosted_review_retention_service.dart';
import 'package:alera/src/features/workbench/application/workbench_pull_request_diff_tab_open_coordinator.dart';
import 'package:alera/src/features/workbench/application/workbench_pull_request_diff_tab_store.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'new pull request tab persists the retention before placement',
    () async {
      final events = <String>[];
      final tab = _pullRequestTab(id: 'tab-new', retentionId: 'retention-new');
      final store = _FakeStore(events: events, result: tab);
      final retention = _FakeRangeRetention(events: events);
      final coordinator = WorkbenchPullRequestDiffTabOpenCoordinator(
        tabStore: store,
        hostedReviewRetention: retention,
      );

      final result = await coordinator.open(
        workspace: _workspace(),
        previousTabs: const <WorkspaceTabRecord>[],
        gitDiffRoot: 'packages/app',
        pullRequestNumber: 385,
        commitOid: 'head-385',
        parentOid: 'base-385',
        retentionId: 'retention-new',
        subject: 'Subject',
        onReady: ({required tab, required alreadyOpen}) async {
          events.add('ready:${tab.id}:$alreadyOpen');
        },
      );

      expect(result, same(tab));
      expect(events, <String>[
        'open:retention-new',
        'persist:retention-new',
        'ready:tab-new:false',
      ]);
      expect(store.closedTabIds, isEmpty);
    },
  );

  test('reused tab releases an incoming retention it does not own', () async {
    final events = <String>[];
    final tab = _pullRequestTab(
      id: 'tab-existing',
      retentionId: 'retention-old',
    );
    final store = _FakeStore(events: events, result: tab);
    final retention = _FakeRangeRetention(events: events);
    final coordinator = WorkbenchPullRequestDiffTabOpenCoordinator(
      tabStore: store,
      hostedReviewRetention: retention,
    );

    await coordinator.open(
      workspace: _workspace(),
      previousTabs: <WorkspaceTabRecord>[tab],
      pullRequestNumber: 385,
      commitOid: 'head-385',
      parentOid: 'base-385',
      retentionId: 'retention-new',
      onReady: ({required tab, required alreadyOpen}) async {
        events.add('ready:${tab.id}:$alreadyOpen');
      },
    );

    expect(events, <String>[
      'open:retention-new',
      'release:retention-new',
      'ready:tab-existing:true',
    ]);
    expect(store.closedTabIds, isEmpty);
  });

  test(
    'persist failure closes a new tab and releases the incoming retention',
    () async {
      final events = <String>[];
      final tab = _pullRequestTab(id: 'tab-new', retentionId: 'retention-new');
      final store = _FakeStore(events: events, result: tab);
      final retention = _FakeRangeRetention(
        events: events,
        persistError: StateError('persist failed'),
      );
      final coordinator = WorkbenchPullRequestDiffTabOpenCoordinator(
        tabStore: store,
        hostedReviewRetention: retention,
      );

      await expectLater(
        coordinator.open(
          workspace: _workspace(),
          previousTabs: const <WorkspaceTabRecord>[],
          pullRequestNumber: 385,
          commitOid: 'head-385',
          parentOid: 'base-385',
          retentionId: 'retention-new',
          onReady: ({required tab, required alreadyOpen}) async {
            events.add('ready');
          },
        ),
        throwsStateError,
      );

      expect(events, <String>[
        'open:retention-new',
        'persist:retention-new',
        'close:tab-new',
        'release:retention-new',
      ]);
    },
  );

  test(
    'placement failure does not discard a retention already owned by the tab',
    () async {
      final events = <String>[];
      final tab = _pullRequestTab(id: 'tab-new', retentionId: 'retention-new');
      final store = _FakeStore(events: events, result: tab);
      final retention = _FakeRangeRetention(events: events);
      final coordinator = WorkbenchPullRequestDiffTabOpenCoordinator(
        tabStore: store,
        hostedReviewRetention: retention,
      );

      await expectLater(
        coordinator.open(
          workspace: _workspace(),
          previousTabs: const <WorkspaceTabRecord>[],
          pullRequestNumber: 385,
          commitOid: 'head-385',
          parentOid: 'base-385',
          retentionId: 'retention-new',
          onReady: ({required tab, required alreadyOpen}) async {
            events.add('ready:${tab.id}:$alreadyOpen');
            throw StateError('layout failed');
          },
        ),
        throwsStateError,
      );

      expect(events, <String>[
        'open:retention-new',
        'persist:retention-new',
        'ready:tab-new:false',
      ]);
      expect(store.closedTabIds, isEmpty);
    },
  );

  test('open failure releases the incoming retention', () async {
    final events = <String>[];
    final store = _FakeStore(
      events: events,
      openError: StateError('open failed'),
    );
    final retention = _FakeRangeRetention(events: events);
    final coordinator = WorkbenchPullRequestDiffTabOpenCoordinator(
      tabStore: store,
      hostedReviewRetention: retention,
    );

    await expectLater(
      coordinator.open(
        workspace: _workspace(),
        previousTabs: const <WorkspaceTabRecord>[],
        pullRequestNumber: 385,
        commitOid: 'head-385',
        parentOid: 'base-385',
        retentionId: 'retention-new',
        onReady: ({required tab, required alreadyOpen}) async {},
      ),
      throwsStateError,
    );

    expect(events, <String>['open:retention-new', 'release:retention-new']);
  });
}

final class _FakeStore implements WorkbenchPullRequestDiffTabStore {
  _FakeStore({required this.events, this.result, this.openError});

  final List<String> events;
  final WorkspaceTabRecord? result;
  final Object? openError;
  final List<String> closedTabIds = <String>[];

  @override
  Future<WorkspaceTabRecord> openOrCreateGitPullRequestDiffTab({
    required String workspaceId,
    String? gitDiffRoot,
    required int pullRequestNumber,
    required String commitOid,
    required String parentOid,
    required String retentionId,
    String? subject,
  }) async {
    events.add('open:$retentionId');
    if (openError case final error?) throw error;
    return result!;
  }

  @override
  Future<void> closeTab(String tabId) async {
    closedTabIds.add(tabId);
    events.add('close:$tabId');
  }
}

final class _FakeRangeRetention implements WorkbenchHostedReviewRangeRetention {
  _FakeRangeRetention({required this.events, this.persistError});

  final List<String> events;
  final Object? persistError;

  @override
  Future<void> persist({
    required Workspace workspace,
    required String? relativeRoot,
    required String retentionId,
  }) async {
    events.add('persist:$retentionId');
    if (persistError case final error?) throw error;
  }

  @override
  Future<void> release({
    required Workspace workspace,
    required String? relativeRoot,
    required String retentionId,
    String? fallbackWorkspacePath,
  }) async {
    events.add('release:$retentionId');
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

WorkspaceTabRecord _pullRequestTab({
  required String id,
  required String retentionId,
}) {
  final now = DateTime.utc(2026, 9, 11);
  return WorkspaceTabRecord(
    id: id,
    workspaceId: 'workspace',
    kind: WorkspaceTabKind.gitDiff,
    title: 'Pull request #385',
    createdAt: now,
    updatedAt: now,
    payload: <String, Object?>{
      workspaceTabGitDiffSourcePayloadKey:
          WorkspaceGitDiffSource.pullRequest.key,
      workspaceTabGitDiffHostedReviewRetentionIdPayloadKey: retentionId,
    },
  );
}
